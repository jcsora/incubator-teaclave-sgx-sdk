// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License..

use crate::session::Initiator;
use crate::QveReportInfo;
use core::mem::{self, ManuallyDrop};
use core::slice;
use sgx_dcap_ra_msg::{DcapMRaMsg2, DcapRaMsg3};
use sgx_trts::fence;
use sgx_trts::trts::is_within_host;
use sgx_types::error::SgxStatus;
use sgx_types::types::time_t;
use sgx_types::types::{
    CDcapMRaMsg2, CDcapRaMsg3, CDcapURaMsg2, Ec256PublicKey, QlQvResult, Quote3, QuoteNonce,
    RaContext, Report, TargetInfo,
};

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn sgx_dcap_ra_get_ga_trusted(
    context: RaContext,
    pub_key_a: *mut Ec256PublicKey,
) -> SgxStatus {
    if pub_key_a.is_null() {
        return SgxStatus::InvalidParameter;
    }

    let initiator = ManuallyDrop::new(Initiator::from_raw(context));
    let key = match initiator.get_ga() {
        Ok(key) => key,
        Err(e) => return e,
    };

    *pub_key_a = key.into();
    SgxStatus::Success
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn sgx_dcap_mra_proc_msg2_trusted(
    context: RaContext,
    msg2: *const CDcapMRaMsg2,
    msg2_size: u32,
    expiration_time: time_t,
    collateral_expiration_status: u32,
    quote_verification_result: QlQvResult,
    qve_nonce: *const QuoteNonce,
    qve_report: *const Report,
    supplemental_data: *const u8,
    supplemental_data_size: u32,
    qe_target: *const TargetInfo,
    report: *mut Report,
    nonce: *mut QuoteNonce,
) -> SgxStatus {
    if msg2.is_null()
        || qve_nonce.is_null()
        || qve_report.is_null()
        || qe_target.is_null()
        || report.is_null()
        || nonce.is_null()
    {
        return SgxStatus::InvalidParameter;
    }

    if supplemental_data.is_null() && supplemental_data_size != 0 {
        return SgxStatus::InvalidParameter;
    }
    if !supplemental_data.is_null() && supplemental_data_size == 0 {
        return SgxStatus::InvalidParameter;
    }

    if usize::MAX - (msg2 as usize) < msg2_size as usize
        || msg2_size < (mem::size_of::<CDcapMRaMsg2>() + mem::size_of::<Quote3>()) as u32
    {
        return SgxStatus::InvalidParameter;
    }

    if !is_within_host(msg2 as *const u8, msg2_size as usize) {
        return SgxStatus::InvalidParameter;
    }

    fence::lfence();

    let qve_nonce = *qve_nonce;
    let qve_report = &*qve_report;
    let qe_target = &*qe_target;

    let msg2_slice = slice::from_raw_parts(msg2 as *const u8, msg2_size as usize);
    let msg2 = match DcapMRaMsg2::from_slice(msg2_slice) {
        Ok(msg) => msg,
        Err(e) => return e,
    };

    let supplemental_data = if !supplemental_data.is_null() {
        Some(slice::from_raw_parts(
            supplemental_data,
            supplemental_data_size as usize,
        ))
    } else {
        None
    };

    let qve_report_info = QveReportInfo {
        qve_report,
        expiration_time,
        collateral_expiration_status,
        quote_verification_result,
        qve_nonce,
        supplemental_data,
    };

    let initiator = ManuallyDrop::new(Initiator::from_raw(context));
    let (rpt, rand, _) = match initiator.process_mra_msg2(&msg2, qe_target, &qve_report_info) {
        Ok(r) => r,
        Err(e) => return e,
    };

    *report = rpt;
    *nonce = rand;
    SgxStatus::Success
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn sgx_dcap_ura_proc_msg2_trusted(
    context: RaContext,
    msg2: *const CDcapURaMsg2,
    qe_target: *const TargetInfo,
    report: *mut Report,
    nonce: *mut QuoteNonce,
) -> SgxStatus {
    if msg2.is_null() || qe_target.is_null() || report.is_null() || nonce.is_null() {
        return SgxStatus::InvalidParameter;
    }

    let qe_target = &*qe_target;
    let msg2 = (&*msg2).into();

    let initiator = ManuallyDrop::new(Initiator::from_raw(context));
    let (rpt, rand) = match initiator.process_ura_msg2(&msg2, qe_target) {
        Ok(r) => r,
        Err(e) => return e,
    };

    *report = rpt;
    *nonce = rand;
    SgxStatus::Success
}

/// # Safety
#[no_mangle]
pub unsafe extern "C" fn sgx_dcap_ra_get_msg3_trusted(
    context: RaContext,
    qe_report: *const Report,
    msg3: *mut CDcapRaMsg3,
    msg3_size: u32,
) -> SgxStatus {
    if qe_report.is_null() || msg3.is_null() {
        return SgxStatus::InvalidParameter;
    }

    if usize::MAX - (msg3 as usize) < msg3_size as usize
        || msg3_size < (mem::size_of::<CDcapRaMsg3>() + mem::size_of::<Quote3>()) as u32
    {
        return SgxStatus::InvalidParameter;
    }

    if !is_within_host(msg3 as *const u8, msg3_size as usize) {
        return SgxStatus::InvalidParameter;
    }

    fence::lfence();

    let qe_report = &*qe_report;
    let c_msg3 = &mut *msg3;
    let quote_size = c_msg3.quote_size;

    if !DcapRaMsg3::check_quote_len(quote_size as usize) {
        return SgxStatus::InvalidParameter;
    }
    if msg3_size != mem::size_of::<CDcapRaMsg3>() as u32 + quote_size {
        return SgxStatus::InvalidParameter;
    }

    let quote = slice::from_raw_parts(&c_msg3.quote as *const _ as *const u8, quote_size as usize);

    let initiator = ManuallyDrop::new(Initiator::from_raw(context));
    let msg3 = match initiator.generate_msg3(qe_report, quote) {
        Ok(msg) => msg,
        Err(e) => return e,
    };

    c_msg3.mac = msg3.mac;
    c_msg3.g_a = msg3.pub_key_a.into();
    SgxStatus::Success
}