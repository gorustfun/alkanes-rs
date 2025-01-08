use crate::message::AlkaneMessageContext;
use crate::tables::TRACES;
use crate::utils::{
    alkane_inventory_pointer, balance_pointer, credit_balances, debit_balances, pipe_storagemap_to,
};
use crate::vm::runtime::AlkanesRuntimeContext;
use crate::network::set_view_mode;
use crate::vm::utils::{prepare_context, run_after_special, run_special_cellpacks};
use alkanes_support::cellpack::Cellpack;
use alkanes_support::id::AlkaneId;
use alkanes_support::parcel::AlkaneTransfer;
use alkanes_support::proto;
use alkanes_support::proto::alkanes::{AlkaneInventoryRequest, AlkaneInventoryResponse};
use alkanes_support::response::ExtendedCallResponse;
use anyhow::Result;
use bitcoin::blockdata::transaction::Version;
use bitcoin::hashes::Hash;
use bitcoin::{
    blockdata::block::Header, Block, BlockHash, CompactTarget, OutPoint, Transaction, TxMerkleNode,
};
use metashrew::index_pointer::{AtomicPointer, IndexPointer};
#[allow(unused_imports)]
use metashrew::{println, stdio::stdout};
use metashrew_support::{index_pointer::KeyValuePointer, utils::consensus_encode};
use protobuf::{Message, MessageField};
use protorune::message::{MessageContext, MessageContextParcel};
use protorune::view;
use protorune_support::balance_sheet::BalanceSheet;
use protorune_support::balance_sheet::ProtoruneRuneId;
use protorune_support::rune_transfer::RuneTransfer;
use protorune_support::utils::{consensus_decode, decode_varint_list};
#[allow(unused_imports)]
use std::fmt::Write;
use std::io::Cursor;
use std::sync::{Arc, Mutex};

pub fn parcel_from_protobuf(v: proto::alkanes::MessageContextParcel) -> MessageContextParcel {
    let mut result = MessageContextParcel::default();
    result.height = v.height;
    result.block = if v.block.len() > 0 {
        consensus_decode::<Block>(&mut Cursor::new(v.block)).unwrap()
    } else {
        default_block()
    };
    result.transaction = if v.transaction.len() > 0 {
        consensus_decode::<Transaction>(&mut Cursor::new(v.transaction)).unwrap()
    } else {
        default_transaction()
    };
    result.vout = v.vout;
    result.calldata = v.calldata;
    result.runes = v
        .alkanes
        .into_iter()
        .map(|v| RuneTransfer {
            id: v.id.into_option().unwrap().clone().into(),
            value: v.value.into_option().unwrap().into(),
        })
        .collect::<Vec<RuneTransfer>>();
    result.pointer = v.pointer;
    result.refund_pointer = v.refund_pointer;
    result
}

fn default_transaction() -> Transaction {
    Transaction {
        version: Version::non_standard(0),
        lock_time: bitcoin::absolute::LockTime::from_consensus(0),
        input: vec![],
        output: vec![],
    }
}

fn default_block() -> Block {
    Block {
        header: Header {
            version: bitcoin::blockdata::block::Version::ONE,
            prev_blockhash: BlockHash::all_zeros(),
            merkle_root: TxMerkleNode::all_zeros(),
            time: 0,
            bits: CompactTarget::from_consensus(0),
            nonce: 0,
        },
        txdata: vec![],
    }
}

pub fn plain_parcel_from_cellpack(cellpack: Cellpack) -> MessageContextParcel {
    let mut result = MessageContextParcel::default();
    result.block = default_block();
    result.transaction = default_transaction();
    result.calldata = cellpack.encipher();
    result
}

pub fn call_view(id: &AlkaneId, inputs: &Vec<u128>, fuel: u64) -> Result<Vec<u8>> {
    let (response, _gas_used) = simulate_parcel(
        &plain_parcel_from_cellpack(Cellpack {
            target: id.clone(),
            inputs: inputs.clone(),
        }),
        fuel,
    )?;
    Ok(response.data)
}

pub const STATIC_FUEL: u64 = 100_000;
pub const NAME_OPCODE: u128 = 99;
pub const SYMBOL_OPCODE: u128 = 100;

pub fn get_statics(id: &AlkaneId) -> (String, String) {
    let name = call_view(id, &vec![NAME_OPCODE], STATIC_FUEL)
        .and_then(|v| Ok(String::from_utf8(v)))
        .unwrap_or_else(|_| Ok(String::from("{REVERT}")))
        .unwrap();
    let symbol = call_view(id, &vec![SYMBOL_OPCODE], STATIC_FUEL)
        .and_then(|v| Ok(String::from_utf8(v)))
        .unwrap_or_else(|_| Ok(String::from("{REVERT}")))
        .unwrap();
    (name, symbol)
}

pub fn to_alkanes_balances(
    balances: protorune_support::proto::protorune::BalanceSheet,
) -> protorune_support::proto::protorune::BalanceSheet {
    let mut clone = balances.clone();
    for entry in &mut clone.entries {
        let block: u128 = entry
            .rune
            .clone()
            .unwrap()
            .runeId
            .height
            .clone()
            .unwrap()
            .into();
        if block == 2 || block == 4 {
            (
                entry.rune.as_mut().unwrap().name,
                entry.rune.as_mut().unwrap().symbol,
            ) = get_statics(&from_protobuf(entry.rune.runeId.clone().unwrap()));
            entry.rune.as_mut().unwrap().spacers = 0;
        }
    }
    clone
}

pub fn to_alkanes_from_runes(
    runes: Vec<protorune_support::proto::protorune::Rune>,
) -> Vec<protorune_support::proto::protorune::Rune> {
    runes
        .into_iter()
        .map(|mut v| {
            let block: u128 = v.clone().runeId.height.clone().unwrap().into();
            if block == 2 || block == 4 {
                (v.name, v.symbol) = get_statics(&from_protobuf(v.runeId.clone().unwrap()));
                v.spacers = 0;
            }
            v
        })
        .collect::<Vec<protorune_support::proto::protorune::Rune>>()
}

pub fn from_protobuf(v: protorune_support::proto::protorune::ProtoruneRuneId) -> AlkaneId {
    let protorune_rune_id: ProtoruneRuneId = v.into();
    protorune_rune_id.into()
}

fn into_u128(v: protorune_support::proto::protorune::Uint128) -> u128 {
    v.into()
}

pub fn protorunes_by_outpoint(
    input: &Vec<u8>,
) -> Result<protorune_support::proto::protorune::OutpointResponse> {
    let request =
        protorune_support::proto::protorune::OutpointWithProtocol::parse_from_bytes(input)?;
    view::protorunes_by_outpoint(input).and_then(|mut response| {
        if into_u128(request.protocol.unwrap_or_else(|| {
            <u128 as Into<protorune_support::proto::protorune::Uint128>>::into(1u128)
        })) == AlkaneMessageContext::protocol_tag()
        {
            response.balances =
                MessageField::some(
                    to_alkanes_balances(response.balances.unwrap_or_else(|| {
                        protorune_support::proto::protorune::BalanceSheet::new()
                    }))
                    .clone(),
                );
        }
        Ok(response)
    })
}

pub fn to_alkanes_outpoints(
    v: Vec<protorune_support::proto::protorune::OutpointResponse>,
) -> Vec<protorune_support::proto::protorune::OutpointResponse> {
    let mut cloned = v.clone();
    for item in &mut cloned {
        item.balances = MessageField::some(
            to_alkanes_balances(
                item.balances
                    .clone()
                    .unwrap_or_else(|| protorune_support::proto::protorune::BalanceSheet::new()),
            )
            .clone(),
        );
    }
    cloned
}

pub fn protorunes_by_address(
    input: &Vec<u8>,
) -> Result<protorune_support::proto::protorune::WalletResponse> {
    let request =
        protorune_support::proto::protorune::ProtorunesWalletRequest::parse_from_bytes(input)?;
    view::protorunes_by_address(input).and_then(|mut response| {
        if into_u128(request.protocol_tag.unwrap_or_else(|| {
            <u128 as Into<protorune_support::proto::protorune::Uint128>>::into(1u128)
        })) == AlkaneMessageContext::protocol_tag()
        {
            response.outpoints = to_alkanes_outpoints(response.outpoints.clone());
        }
        Ok(response)
    })
}

pub fn protorunes_by_height(
    input: &Vec<u8>,
) -> Result<protorune_support::proto::protorune::RunesResponse> {
    let request =
        protorune_support::proto::protorune::ProtorunesByHeightRequest::parse_from_bytes(input)?;
    view::protorunes_by_height(input).and_then(|mut response| {
        if into_u128(request.protocol_tag.unwrap_or_else(|| {
            <u128 as Into<protorune_support::proto::protorune::Uint128>>::into(1u128)
        })) == AlkaneMessageContext::protocol_tag()
        {
            response.runes = to_alkanes_from_runes(response.runes.clone());
        }
        Ok(response)
    })
}

pub fn alkane_inventory(req: &AlkaneInventoryRequest) -> Result<AlkaneInventoryResponse> {
    let mut result: AlkaneInventoryResponse = AlkaneInventoryResponse::new();
    let alkane_inventory = alkane_inventory_pointer(&req.id.clone().unwrap().into());
    result.alkanes = alkane_inventory
        .get_list()
        .into_iter()
        .map(|alkane_held| -> proto::alkanes::AlkaneTransfer {
            let id = alkanes_support::id::AlkaneId::parse(&mut Cursor::new(
                alkane_held.as_ref().clone(),
            ))
            .unwrap();
            let balance_pointer = balance_pointer(
                &mut AtomicPointer::default(),
                &req.id.clone().unwrap().into(),
                &id,
            );
            let balance = balance_pointer.get_value::<u128>();
            (AlkaneTransfer {
                id: id,
                value: balance,
            })
            .into()
        })
        .collect::<Vec<proto::alkanes::AlkaneTransfer>>();
    Ok(result)
}

pub fn trace(outpoint: &OutPoint) -> Result<Vec<u8>> {
    Ok(TRACES
        .select(&consensus_encode::<OutPoint>(&outpoint)?)
        .get()
        .as_ref()
        .clone())
}

pub fn simulate_safe(
  parcel: &MessageContextParcel,
  fuel: u64
) -> Result<(ExtendedCallResponse, u64)> {
  set_view_mode();
  simulate_parcel(parcel, fuel)
}

pub fn simulate_parcel(
    parcel: &MessageContextParcel,
    fuel: u64,
) -> Result<(ExtendedCallResponse, u64)> {
    let cellpack: Cellpack =
        decode_varint_list(&mut Cursor::new(parcel.calldata.clone()))?.try_into()?;
    let context = Arc::new(Mutex::new(AlkanesRuntimeContext::from_parcel_and_cellpack(
        parcel, &cellpack,
    )));
    let mut atomic = parcel.atomic.derive(&IndexPointer::default());
    let (caller, myself, binary) = run_special_cellpacks(context.clone(), &cellpack)?;
    credit_balances(&mut atomic, &myself, &parcel.runes);
    prepare_context(context.clone(), &caller, &myself, false);
    let (response, gas_used) = run_after_special(context.clone(), binary, fuel)?;
    pipe_storagemap_to(
        &response.storage,
        &mut atomic.derive(&IndexPointer::from_keyword("/alkanes/").select(&myself.clone().into())),
    );
    let mut combined = parcel.runtime_balances.as_ref().clone();
    <BalanceSheet as From<Vec<RuneTransfer>>>::from(parcel.runes.clone()).pipe(&mut combined);
    let sheet = <BalanceSheet as From<Vec<RuneTransfer>>>::from(response.alkanes.clone().into());
    combined.debit(&sheet)?;
    debit_balances(&mut atomic, &myself, &response.alkanes)?;
    Ok((response, gas_used))
}
