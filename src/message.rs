use crate::network::{genesis::GENESIS_BLOCK, is_active};
use crate::utils::{credit_balances, debit_balances, pipe_storagemap_to};
use crate::trace::{save_trace};
use crate::vm::{
    fuel::start_fuel,
    runtime::AlkanesRuntimeContext,
    utils::{prepare_context, run_after_special, run_special_cellpacks},
};
use alkanes_support::{response::{ExtendedCallResponse}, trace::{TraceEvent, TraceResponse, TraceContext}, cellpack::Cellpack};
use anyhow::{anyhow, Result};
use metashrew::index_pointer::IndexPointer;
#[allow(unused_imports)]
use metashrew::{
    println,
    stdio::{stdout, Write},
};
use metashrew_support::index_pointer::KeyValuePointer;
use protorune::balance_sheet::MintableDebit;
use protorune::message::{MessageContext, MessageContextParcel};
use protorune_support::{
    balance_sheet::BalanceSheet, rune_transfer::RuneTransfer, utils::decode_varint_list,
};
use std::io::Cursor;
use bitcoin::{OutPoint};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct AlkaneMessageContext(());

// TODO: import MessageContextParcel

pub fn handle_message(parcel: &MessageContextParcel) -> Result<(Vec<RuneTransfer>, BalanceSheet)> {
    let cellpack: Cellpack =
        decode_varint_list(&mut Cursor::new(parcel.calldata.clone()))?.try_into()?;
    let target = cellpack.target.clone();
    let mut context = Arc::new(Mutex::new(AlkanesRuntimeContext::from_parcel_and_cellpack(parcel, &cellpack)));
    let mut atomic = parcel.atomic.derive(&IndexPointer::default());
    let (caller, myself, binary) = run_special_cellpacks(context.clone(), &cellpack)?;
    credit_balances(&mut atomic, &myself, &parcel.runes);
    prepare_context(context.clone(), &caller, &myself, false);
    let fuel = start_fuel();
    let inner = context.lock().unwrap().flat();
    let trace = context.lock().unwrap().trace.clone();
    trace.clock(TraceEvent::EnterCall(TraceContext {
      inner,
      target,
      fuel
    }));
    run_after_special(context.clone(), binary, fuel).and_then(|(response, _gas_used)| {
      pipe_storagemap_to(
          &response.storage,
          &mut atomic.derive(&IndexPointer::from_keyword("/alkanes/").select(&myself.clone().into())),
      );
      let mut combined = parcel.runtime_balances.as_ref().clone();
      <BalanceSheet as From<Vec<RuneTransfer>>>::from(parcel.runes.clone()).pipe(&mut combined);
      let sheet = <BalanceSheet as From<Vec<RuneTransfer>>>::from(response.alkanes.clone().into());
      combined.debit_mintable(&sheet, &mut atomic)?;
      debit_balances(&mut atomic, &myself, &response.alkanes)?;
      save_trace(&OutPoint {
        txid: parcel.transaction.compute_txid(),
        vout: parcel.vout
      }, parcel.height, trace.clone())?;
      Ok((response.alkanes.into(), combined))
    }).or_else(|e| {
      let mut response = ExtendedCallResponse::default();
      
      response.data = vec![0x08, 0xc3, 0x79, 0xa0];
      response.data.extend(e.to_string().as_bytes());
      let mut cloned = context.clone().lock().unwrap().trace.clone();
      cloned.clock(TraceEvent::RevertContext(TraceResponse {
        inner: response,
        fuel_used: u64::MAX,
      }));
      save_trace(&OutPoint {
        txid: parcel.transaction.compute_txid(),
        vout: parcel.vout
      }, parcel.height, cloned)?;
      Err(e)
    })
}

impl MessageContext for AlkaneMessageContext {
    fn protocol_tag() -> u128 {
        1
    }
    fn handle(_parcel: &MessageContextParcel) -> Result<(Vec<RuneTransfer>, BalanceSheet)> {
        if is_active(_parcel.height) {
            match handle_message(_parcel) {
                Ok((outgoing, runtime)) => Ok((outgoing, runtime)),
                Err(e) => {
                    println!("{:?}", e);
                    Err(e) // Print the error
                }
            }
        } else {
            Err(anyhow!(
                "subprotocol inactive until block {}",
                GENESIS_BLOCK
            ))
        }
    }
}
