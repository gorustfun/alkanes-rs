pub mod auth;
#[cfg(feature = "panic-hook")]
pub mod compat;
pub mod imports;
pub mod message;
pub mod runtime;
pub mod stdio;
pub mod storage;
pub mod token;
pub use crate::stdio::stdout;

#[macro_export]
macro_rules! declare_alkane {
    ($struct_name:ident) => {
        #[no_mangle]
        pub extern "C" fn __execute() -> i32 {
            let mut response = to_arraybuffer_layout(&$struct_name::default().run());
            Box::leak(Box::new(response)).as_mut_ptr() as usize as i32 + 4
        }
    };

    (impl AlkaneResponder for $struct_name:ident {
        type Message = $message_type:ident;
    }) => {
        #[no_mangle]
        pub extern "C" fn __execute() -> i32 {
            use alkanes_runtime::runtime::AlkaneResponder;
            use alkanes_runtime::runtime::{handle_error, handle_success, prepare_response};
            use metashrew_support::compat::{to_arraybuffer_layout, to_passback_ptr};

            let mut context = $struct_name::default().context().unwrap();
            let mut inputs = context.inputs.clone();

            if inputs.is_empty() {
                // Use the handle_error helper function
                let extended = handle_error("No opcode provided");
                return alkanes_runtime::runtime::response_to_i32(extended);
            }

            let opcode = inputs[0];
            inputs.remove(0);

            let result = match $message_type::from_opcode(opcode, inputs) {
                Ok(message) => message.dispatch(&$struct_name::default()),
                Err(err) => Err(anyhow::anyhow!("Failed to parse message: {}", err)),
            };

            let extended = match result {
                Ok(res) => {
                    // Use the handle_success helper function
                    handle_success(res)
                }
                Err(err) => {
                    // Use the handle_error helper function
                    let error_msg = format!("Error: {}", err);
                    let extended = handle_error(&error_msg);
                    return alkanes_runtime::runtime::response_to_i32(extended);
                }
            };

            // Use the response_to_i32 helper function
            alkanes_runtime::runtime::response_to_i32(extended)
        }

        #[no_mangle]
        pub extern "C" fn __meta() -> i32 {
            let abi = $message_type::export_abi();
            export_bytes(&abi)
        }

        fn export_bytes(data: &[u8]) -> i32 {
            // Use the to_arraybuffer_layout function directly for raw bytes
            let response_bytes = to_arraybuffer_layout(data);
            Box::leak(Box::new(response_bytes)).as_mut_ptr() as usize as i32 + 4
        }
    };
}
