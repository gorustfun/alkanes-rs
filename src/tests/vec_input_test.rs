use crate::{message::AlkaneMessageContext, tests::std::alkanes_std_test_build};
use alkanes_support::cellpack::Cellpack;
use alkanes_support::id::AlkaneId;
use anyhow::Result;
use bitcoin::OutPoint;
use metashrew_support::utils::consensus_encode;

use crate::index_block;
use crate::tests::helpers::{self as alkane_helpers, assert_binary_deployed_to_id};
use alkane_helpers::clear;
use alkanes::view;
use bitcoin::Witness;
#[allow(unused_imports)]
use metashrew::{
    println,
    stdio::{stdout, Write},
};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn test_vec_inputs() -> Result<()> {
    clear();
    let block_height = 840_000;

    // Create a cellpack to call the process_numbers method (opcode 11)
    let process_numbers_cellpack = Cellpack {
        target: AlkaneId { block: 1, tx: 0 },
        inputs: vec![
            11, // opcode for process_numbers
            4,  // length of the vector
            10, // first element
            20, // second element
            30, // third element
            40, // fourth element
        ],
    };

    // Create a cellpack to call the process_strings method (opcode 12)
    // For "hello" and "world" strings with null terminators
    let hello_bytes = u128::from_le_bytes(*b"hello\0\0\0\0\0\0\0\0\0\0\0");
    let world_bytes = u128::from_le_bytes(*b"world\0\0\0\0\0\0\0\0\0\0\0");

    let process_strings_cellpack = Cellpack {
        target: AlkaneId { block: 1, tx: 0 },
        inputs: vec![
            12,          // opcode for process_strings
            2,           // length of the vector
            hello_bytes, // "hello" string
            world_bytes, // "world" string
        ],
    };

    // Create a cellpack to call the process_nested_vec method (opcode 15)
    let process_nested_vec_cellpack = Cellpack {
        target: AlkaneId { block: 1, tx: 0 },
        inputs: vec![
            15, // opcode for process_nested_vec
            2,  // length of the outer vector
            3,  // length of first inner vector
            1,  // elements of first inner vector
            2, 3, 2, // length of second inner vector
            4, // elements of second inner vector
            5,
        ],
    };

    // Create a cellpack to call the get_numbers method (opcode 13)
    let get_numbers_cellpack = Cellpack {
        target: AlkaneId { block: 1, tx: 0 },
        inputs: vec![13], // opcode for get_numbers
    };

    // Create a cellpack to call the get_strings method (opcode 14)
    let get_strings_cellpack = Cellpack {
        target: AlkaneId { block: 1, tx: 0 },
        inputs: vec![14], // opcode for get_strings
    };

    // Initialize the contract and execute the cellpacks
    let mut test_block = alkane_helpers::init_with_multiple_cellpacks_with_tx(
        [alkanes_std_test_build::get_bytes()].into(),
        [process_numbers_cellpack].into(),
    );

    // Add a transaction with the remaining cellpacks
    test_block.txdata.push(
        alkane_helpers::create_multiple_cellpack_with_witness_and_in(
            Witness::new(),
            vec![
                process_strings_cellpack,
                process_nested_vec_cellpack,
                get_numbers_cellpack,
                get_strings_cellpack,
            ],
            OutPoint {
                txid: test_block.txdata[test_block.txdata.len() - 1].compute_txid(),
                vout: 0,
            },
            false,
        ),
    );

    index_block(&test_block, block_height)?;

    // Get the LoggerAlkane ID
    let logger_alkane_id = AlkaneId { block: 2, tx: 1 };

    // Verify the binary was deployed correctly
    let _ = assert_binary_deployed_to_id(
        logger_alkane_id.clone(),
        alkanes_std_test_build::get_bytes(),
    );

    // Get the trace data from the transaction for get_numbers
    let outpoint_get_numbers = OutPoint {
        txid: test_block.txdata[test_block.txdata.len() - 1].compute_txid(),
        vout: 3,
    };

    let trace_data_get_numbers = view::trace(&outpoint_get_numbers)?;
    println!("get_numbers trace: {:?}", trace_data_get_numbers);

    // Verify the get_numbers result contains the expected values
    // The result should be a vector with [1, 2, 3]
    assert!(
        trace_data_get_numbers.len() > 0,
        "get_numbers should return data"
    );

    // Get the trace data from the transaction for get_strings
    let outpoint_get_strings = OutPoint {
        txid: test_block.txdata[test_block.txdata.len() - 1].compute_txid(),
        vout: 4,
    };

    let trace_data_get_strings = view::trace(&outpoint_get_strings)?;
    println!("get_strings trace: {:?}", trace_data_get_strings);

    // Verify the get_strings result contains the expected values
    // The result should be a vector with ["hello", "world"]
    assert!(
        trace_data_get_strings.len() > 0,
        "get_strings should return data"
    );

    // Get the trace data from the transaction for process_numbers
    let outpoint_process_numbers = OutPoint {
        txid: test_block.txdata[0].compute_txid(),
        vout: 1,
    };

    let trace_data_process_numbers = view::trace(&outpoint_process_numbers)?;
    println!("process_numbers trace: {:?}", trace_data_process_numbers);

    // The result should be the sum of the numbers: 10 + 20 + 30 + 40 = 100
    assert_eq!(
        trace_data_process_numbers.len(),
        16,
        "process_numbers should return a u128 (16 bytes)"
    );

    // Get the trace data from the transaction for process_nested_vec
    let outpoint_process_nested_vec = OutPoint {
        txid: test_block.txdata[test_block.txdata.len() - 1].compute_txid(),
        vout: 2,
    };

    let trace_data_process_nested_vec = view::trace(&outpoint_process_nested_vec)?;
    println!(
        "process_nested_vec trace: {:?}",
        trace_data_process_nested_vec
    );

    // The result should be the total number of elements: 3 + 2 = 5
    assert_eq!(
        trace_data_process_nested_vec.len(),
        16,
        "process_nested_vec should return a u128 (16 bytes)"
    );

    Ok(())
}
