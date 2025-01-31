use alkanes_runtime::runtime::AlkaneResponder;
use alkanes_runtime::{auth::AuthenticatedResponder, declare_alkane};
#[allow(unused_imports)]
use alkanes_runtime::{
    println,
    stdio::{stdout, Write},
};
use alkanes_support::utils::shift_or_err;
use alkanes_support::{parcel::AlkaneTransfer, response::CallResponse};
use anyhow::{anyhow, Result};
use metashrew_support::compat::{to_arraybuffer_layout, to_passback_ptr};
pub mod factory;

use crate::factory::MintableToken;

#[derive(Default)]
pub struct OwnedToken(());

impl MintableToken for OwnedToken {}

impl AuthenticatedResponder for OwnedToken {}

impl AlkaneResponder for OwnedToken {
    fn execute(&self) -> Result<CallResponse> {
        let context = self.context()?;
        let mut inputs = context.inputs.clone();
        let mut response: CallResponse = CallResponse::forward(&context.incoming_alkanes.clone());
        match shift_or_err(&mut inputs)? {
            0 => {
                self.observe_initialization()?;
                println!("owned token initializing");
                let auth_token_units = shift_or_err(&mut inputs)?;
                let token_units = shift_or_err(&mut inputs)?;
                response
                    .alkanes
                    .0
                    .push(self.deploy_auth_token(auth_token_units)?);
                response.alkanes.0.push(AlkaneTransfer {
                    id: context.myself.clone(),
                    value: token_units,
                });
                Ok(response)
            }
            77 => {
                println!("owned token minting");
                self.only_owner()?;
                let token_units = shift_or_err(&mut inputs)?;
                let transfer = self.mint(&context, token_units)?;
                response.alkanes.0.push(transfer);
                Ok(response)
            }
            99 => {
                response.data = self.name().into_bytes().to_vec();
                Ok(response)
            }
            100 => {
                response.data = self.symbol().into_bytes().to_vec();
                Ok(response)
            }
            101 => {
                response.data = self.total_supply().to_le_bytes().to_vec();
                Ok(response)
            }
            1000 => {
                response.data = self.data();
                Ok(response)
            }
            _ => Err(anyhow!("unrecognized opcode")),
        }
    }
}

declare_alkane! {OwnedToken}
