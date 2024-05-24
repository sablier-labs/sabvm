use revm_interpreter::CallOutcome;
use revm_interpreter::CreateOutcome;

use crate::{
    inspectors::GasInspector,
    interpreter::{opcode, CallInputs, CreateInputs, Interpreter},
    Database, EvmContext, Inspector,
};

/// Custom print [Inspector], it has step level information of execution.
///
/// It is a great tool if some debugging is needed.
#[derive(Clone, Debug, Default)]
pub struct CustomPrintTracer {
    gas_inspector: GasInspector,
}

impl<DB: Database> Inspector<DB> for CustomPrintTracer {
    fn initialize_interp(&mut self, interp: &mut Interpreter, context: &mut EvmContext<DB>) {
        self.gas_inspector.initialize_interp(interp, context);
    }

    // get opcode by calling `interp.contract.opcode(interp.program_counter())`.
    // all other information can be obtained from interp.
    fn step(&mut self, interp: &mut Interpreter, context: &mut EvmContext<DB>) {
        let opcode = interp.current_opcode();
        let opcode_str = opcode::OPCODE_JUMPMAP[opcode as usize];

        let gas_remaining = self.gas_inspector.gas_remaining();

        let memory_size = interp.shared_memory.len();

        println!(
            "depth:{}, PC:{}, gas:{:#x}({}), OPCODE: {:?}({:?})  refund:{:#x}({}) Stack:{:?}, Data size:{}",
            context.journaled_state.depth(),
            interp.program_counter(),
            gas_remaining,
            gas_remaining,
            opcode_str.unwrap_or("UNKNOWN"),
            opcode,
            interp.gas.refunded(),
            interp.gas.refunded(),
            interp.stack.data(),
            memory_size,
        );

        self.gas_inspector.step(interp, context);
    }

    fn step_end(&mut self, interp: &mut Interpreter, context: &mut EvmContext<DB>) {
        self.gas_inspector.step_end(interp, context);
    }

    fn call_end(
        &mut self,
        context: &mut EvmContext<DB>,
        inputs: &CallInputs,
        outcome: CallOutcome,
    ) -> CallOutcome {
        self.gas_inspector.call_end(context, inputs, outcome)
    }

    fn create_end(
        &mut self,
        context: &mut EvmContext<DB>,
        inputs: &CreateInputs,
        outcome: CreateOutcome,
    ) -> CreateOutcome {
        self.gas_inspector.create_end(context, inputs, outcome)
    }

    fn call(
        &mut self,
        _context: &mut EvmContext<DB>,
        inputs: &mut CallInputs,
    ) -> Option<CallOutcome> {
        println!(
            "SM CALL:   {:?}, context:{:?}, is_static:{:?}, transfer:{:?}, input_size:{:?}",
            inputs.contract,
            inputs.context,
            inputs.is_static,
            inputs.transfer,
            inputs.input.len(),
        );
        None
    }

    fn create(
        &mut self,
        _context: &mut EvmContext<DB>,
        inputs: &mut CreateInputs,
    ) -> Option<CreateOutcome> {
        println!(
            "CREATE CALL: caller:{:?}, scheme:{:?}, value:{:?}, init_code:{:?}, gas:{:?}",
            inputs.caller,
            inputs.scheme,
            inputs.transferred_assets,
            inputs.init_code,
            inputs.gas_limit
        );
        None
    }
}

#[cfg(test)]
mod test {
    use revm_interpreter::Host;
    use revm_precompile::HashMap;

    use crate::{
        inspector_handle_register,
        inspectors::CustomPrintTracer,
        primitives::{address, bytes, SpecId},
        Evm, InMemoryDB,
    };

    #[test]
    fn gas_calculation_underflow() {
        use crate::primitives::{
            address, bytes, keccak256, AccountInfo, Asset, Bytecode, Bytes, TransactTo,
            BASE_ASSET_ID, U256,
        };
        let callee = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");

        // https://github.com/bluealloy/revm/issues/277
        // checks this use case
        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                let code = bytes!("5b597fb075978b6c412c64d169d56d839a8fe01b3f4607ed603b2c78917ce8be1430fe6101e8527ffe64706ecad72a2f5c97a95e006e279dc57081902029ce96af7edae5de116fec610208527f9fc1ef09d4dd80683858ae3ea18869fe789ddc365d8d9d800e26c9872bac5e5b6102285260276102485360d461024953601661024a53600e61024b53607d61024c53600961024d53600b61024e5360b761024f5360596102505360796102515360a061025253607261025353603a6102545360fb61025553601261025653602861025753600761025853606f61025953601761025a53606161025b53606061025c5360a661025d53602b61025e53608961025f53607a61026053606461026153608c6102625360806102635360d56102645360826102655360ae61026653607f6101e8610146610220677a814b184591c555735fdcca53617f4d2b9134b29090c87d01058e27e962047654f259595947443b1b816b65cdb6277f4b59c10a36f4e7b8658f5a5e6f5561");
                let info = AccountInfo {
                    balances: HashMap::from([(BASE_ASSET_ID, "0x100c5d668240db8e00".parse().unwrap())]),
                    code_hash: keccak256(&code),
                    code: Some(Bytecode::new_raw(code.clone())),
                    nonce: 1,
                };
                db.insert_account_info(callee, info);
            })
            .modify_tx_env(|tx| {
                tx.caller = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");
                tx.transact_to = TransactTo::Call(callee);
                tx.data = Bytes::new();
                tx.transferred_assets = vec![(Asset{ id: BASE_ASSET_ID, amount: U256::ZERO})];
            })
            .with_external_context(CustomPrintTracer::default())
            .with_spec_id(SpecId::BERLIN)
            .append_handler_register(inspector_handle_register)
            .build();

        evm.transact().expect("Transaction to work");
    }

    #[test]
    fn transfer_base_asset() {
        use crate::primitives::{
            address, AccountInfo, Asset, Bytes, TransactTo, B256, BASE_ASSET_ID, U256,
        };
        let callee_eoa = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");
        let caller_eoa = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");

        let mut db = InMemoryDB::default();
        let mut evm = Evm::builder()
            .with_db(db)
            .modify_db(|db| {
                let callee_info = AccountInfo {
                    balances: HashMap::from([(BASE_ASSET_ID, U256::ZERO)]),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(callee_eoa, callee_info);

                let caller_info = AccountInfo {
                    balances: HashMap::from([(BASE_ASSET_ID, U256::from(10))]),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(caller_eoa, caller_info);
            })
            .modify_tx_env(|tx| {
                tx.caller = caller_eoa;
                tx.transact_to = TransactTo::Call(callee_eoa);
                tx.data = Bytes::new();
                tx.transferred_assets = vec![
                    (Asset {
                        id: BASE_ASSET_ID,
                        amount: U256::from(10),
                    }),
                ];
            })
            .with_external_context(CustomPrintTracer::default())
            .with_spec_id(SpecId::BERLIN)
            .append_handler_register(inspector_handle_register)
            .build();

        evm.transact_commit().expect("Transaction to work");

        db = evm.db().clone();
        let callee_base_balance = *db
            .accounts
            .get(&caller_eoa)
            .unwrap()
            .info
            .balances
            .get(&BASE_ASSET_ID)
            .unwrap();
        assert_eq!(callee_base_balance, U256::from(10));
    }

    #[test]
    fn mint_native_asset() {
        use crate::primitives::{
            address, asset_id_address, bytes, keccak256, AccountInfo, Bytecode, U256,
        };
        let caller_contract = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                let code = bytes!("5b597fb075978b6c412c64d169d56d839a8fe01b3f4607ed603b2c78917ce8be1430fe6101e8527ffe64706ecad72a2f5c97a95e006e279dc57081902029ce96af7edae5de116fec610208527f9fc1ef09d4dd80683858ae3ea18869fe789ddc365d8d9d800e26c9872bac5e5b6102285260276102485360d461024953601661024a53600e61024b53607d61024c53600961024d53600b61024e5360b761024f5360596102505360796102515360a061025253607261025353603a6102545360fb61025553601261025653602861025753600761025853606f61025953601761025a53606161025b53606061025c5360a661025d53602b61025e53608961025f53607a61026053606461026153608c6102625360806102635360d56102645360826102655360ae61026653607f6101e8610146610220677a814b184591c555735fdcca53617f4d2b9134b29090c87d01058e27e962047654f259595947443b1b816b65cdb6277f4b59c10a36f4e7b8658f5a5e6f5561");
                let caller_info = AccountInfo {
                    balances: HashMap::new(),
                    code_hash: keccak256(&code),
                    code: Some(Bytecode::new_raw(code.clone())),
                    nonce: 1,
                };
                db.insert_account_info(caller_contract, caller_info);
            })
            .build();

        // Test the minting of a native asset
        let sub_id = U256::from(2);
        let amount_to_mint = U256::from(1000);
        assert!(evm.mint(caller_contract, caller_contract, sub_id, amount_to_mint));

        let minted_asset_id = asset_id_address(caller_contract, sub_id);

        let journaled_state = evm.context.evm.inner.journaled_state.clone();
        let caller_minted_asset_balance = *journaled_state
            .account(caller_contract)
            .info
            .balances
            .get(&minted_asset_id)
            .unwrap();

        assert_eq!(caller_minted_asset_balance, U256::from(1000));
    }

    #[test]
    fn burn_native_asset() {
        use crate::primitives::{
            address, asset_id_address, bytes, keccak256, AccountInfo, Bytecode, U256,
        };
        let caller_contract = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");
        let sub_id = U256::from(2);
        let minted_asset_id = asset_id_address(caller_contract, sub_id);
        let caller_initial_balance = U256::from(1000);

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                db.asset_ids.push(minted_asset_id);

                let code = bytes!("5b597fb075978b6c412c64d169d56d839a8fe01b3f4607ed603b2c78917ce8be1430fe6101e8527ffe64706ecad72a2f5c97a95e006e279dc57081902029ce96af7edae5de116fec610208527f9fc1ef09d4dd80683858ae3ea18869fe789ddc365d8d9d800e26c9872bac5e5b6102285260276102485360d461024953601661024a53600e61024b53607d61024c53600961024d53600b61024e5360b761024f5360596102505360796102515360a061025253607261025353603a6102545360fb61025553601261025653602861025753600761025853606f61025953601761025a53606161025b53606061025c5360a661025d53602b61025e53608961025f53607a61026053606461026153608c6102625360806102635360d56102645360826102655360ae61026653607f6101e8610146610220677a814b184591c555735fdcca53617f4d2b9134b29090c87d01058e27e962047654f259595947443b1b816b65cdb6277f4b59c10a36f4e7b8658f5a5e6f5561");
                let caller_info = AccountInfo {
                    balances: HashMap::from([(minted_asset_id, caller_initial_balance)]),
                    code_hash: keccak256(&code),
                    code: Some(Bytecode::new_raw(code.clone())),
                    nonce: 1,
                };
                db.insert_account_info(caller_contract, caller_info);
            })
            .build();

        // Test the burning of the native asset
        let amount_to_burn = U256::from(500);
        assert!(evm.burn(caller_contract, sub_id, amount_to_burn));

        let journaled_state = evm.context.evm.inner.journaled_state.clone();
        let caller_minted_asset_balance = *journaled_state
            .account(caller_contract)
            .info
            .balances
            .get(&minted_asset_id)
            .unwrap();

        let expected_remaining_balance = caller_initial_balance - amount_to_burn;
        assert_eq!(caller_minted_asset_balance, expected_remaining_balance);
    }

    #[test]
    fn ask_precompile_for_balanceof() {
        use crate::primitives::{
            address, AccountInfo, Bytes, TransactTo, B256, BASE_ASSET_ID, U256,
        };
        use crate::sabvm_precompile::ADDRESS as SABVM_PRECOMPILE_ADDRESS;

        let caller = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");
        let caller_balance = U256::from(10);

        let db = InMemoryDB::default();
        let mut evm = Evm::builder()
            .with_db(db)
            .modify_db(|db| {
                let callee_info = AccountInfo {
                    balances: HashMap::from([(BASE_ASSET_ID, U256::ZERO)]),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(SABVM_PRECOMPILE_ADDRESS, callee_info);

                let caller_info = AccountInfo {
                    balances: HashMap::from([(BASE_ASSET_ID, caller_balance)]),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(caller, caller_info);
            })
            .modify_tx_env(|tx| {
                tx.caller = caller;
                tx.transact_to = TransactTo::Call(SABVM_PRECOMPILE_ADDRESS);

                //Compose the Tx Data, as follows: the BALANCEOF id + address + asset_id
                const BALANCEOF_ID: u8 = 0x2E;
                let mut concatenated = vec![BALANCEOF_ID];
                concatenated.append(caller.to_vec().as_mut());
                concatenated.append(BASE_ASSET_ID.to_be_bytes_vec().as_mut());
                tx.data = Bytes::from(concatenated);
            })
            .with_external_context(CustomPrintTracer::default())
            .with_spec_id(SpecId::LATEST)
            .append_handler_register(inspector_handle_register)
            .build();

        let tx_result = evm.transact_commit();
        assert!(tx_result.is_ok());

        let execution_result = tx_result.unwrap();
        assert!(execution_result.is_success());

        let tx_output = execution_result.output().unwrap();
        let balance = U256::from_be_bytes::<32>(tx_output.to_vec().try_into().unwrap());
        assert_eq!(balance, caller_balance);
    }

    #[test]
    fn ask_precompile_to_mint() {
        use crate::sabvm_precompile::ADDRESS as SABVM_PRECOMPILE_ADDRESS;

        use crate::primitives::{
            address, asset_id_address, bytes, keccak256, AccountInfo, Bytecode, Bytes, TransactTo, U256,
        };
        let caller_contract = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");
        let sub_id = U256::from(2);
        let amount_to_mint = U256::from(1000);

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                let code = bytes!("5b597fb075978b6c412c64d169d56d839a8fe01b3f4607ed603b2c78917ce8be1430fe6101e8527ffe64706ecad72a2f5c97a95e006e279dc57081902029ce96af7edae5de116fec610208527f9fc1ef09d4dd80683858ae3ea18869fe789ddc365d8d9d800e26c9872bac5e5b6102285260276102485360d461024953601661024a53600e61024b53607d61024c53600961024d53600b61024e5360b761024f5360596102505360796102515360a061025253607261025353603a6102545360fb61025553601261025653602861025753600761025853606f61025953601761025a53606161025b53606061025c5360a661025d53602b61025e53608961025f53607a61026053606461026153608c6102625360806102635360d56102645360826102655360ae61026653607f6101e8610146610220677a814b184591c555735fdcca53617f4d2b9134b29090c87d01058e27e962047654f259595947443b1b816b65cdb6277f4b59c10a36f4e7b8658f5a5e6f5561");
                let caller_info = AccountInfo {
                    balances: HashMap::new(),
                    code_hash: keccak256(&code),
                    code: Some(Bytecode::new_raw(code.clone())),
                    nonce: 1,
                };
                db.insert_account_info(caller_contract, caller_info);
            })
            .modify_tx_env(|tx| {
                tx.caller = caller_contract;
                tx.transact_to = TransactTo::Call(SABVM_PRECOMPILE_ADDRESS);

                //Compose the Tx Data, as follows: the BALANCEOF id + address + asset_id
                const MINT_ID: u8 = 0xC0;
                let recipient = caller_contract;
                
                let mut concatenated = vec![MINT_ID];
                concatenated.append(recipient.to_vec().as_mut());
                concatenated.append(sub_id.to_be_bytes_vec().as_mut());
                concatenated.append(amount_to_mint.to_be_bytes_vec().as_mut());
                tx.data = Bytes::from(concatenated);
            }).build();

        let tx_result = evm.transact_commit();
        assert!(tx_result.is_ok());

        let execution_result = tx_result.unwrap();
        assert!(execution_result.is_success());

        let minted_asset_id = asset_id_address(caller_contract, sub_id);
        let caller_minted_asset_balance = evm.balance(minted_asset_id, caller_contract).unwrap().0;
        assert_eq!(caller_minted_asset_balance, amount_to_mint);
    }
}
