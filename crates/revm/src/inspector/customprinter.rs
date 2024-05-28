//! Custom print inspector, it has step level information of execution.
//! It is a great tool if some debugging is needed.

use revm_interpreter::CallOutcome;
use revm_interpreter::CreateOutcome;
use revm_interpreter::OpCode;

use crate::{
    inspectors::GasInspector,
    interpreter::{CallInputs, CreateInputs, Interpreter},
    primitives::{Address, U256},
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
        let name = OpCode::name_by_op(opcode);

        let gas_remaining = self.gas_inspector.gas_remaining();

        let memory_size = interp.shared_memory.len();

        println!(
            "depth:{}, PC:{}, gas:{:#x}({}), OPCODE: {:?}({:?})  refund:{:#x}({}) Stack:{:?}, Data size:{}",
            context.journaled_state.depth(),
            interp.program_counter(),
            gas_remaining,
            gas_remaining,
            name,
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
            "SM Address: {:?}, caller:{:?},target:{:?} is_static:{:?}, transfer:{:?}, input_size:{:?}",
            inputs.bytecode_address,
            inputs.caller,
            inputs.target_address,
            inputs.is_static,
            inputs.value,
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
            inputs.caller, inputs.scheme, inputs.value, inputs.init_code, inputs.gas_limit
        );
        None
    }

    fn selfdestruct(&mut self, contract: Address, target: Address, value: U256) {
        println!(
            "SELFDESTRUCT: contract: {:?}, refund target: {:?}, value {:?}",
            contract, target, value
        );
    }
}

// TODO: move the precompile tests somewhere else
#[cfg(test)]
mod test {
    use crate::{
        inspector_handle_register,
        inspectors::CustomPrintTracer,
        primitives::{
            address, bytes, keccak256, token_id_address, AccountInfo, Bytecode, Bytes, SpecId,
            TokenTransfer, TransactTo, B256, BASE_TOKEN_ID, U256,
        },
        sablier::native_tokens::ADDRESS as NATIVE_TOKENS_PRECOMPILE_ADDRESS,
        Evm, InMemoryDB,
    };
    use revm_interpreter::Host;
    use revm_precompile::HashMap;

    #[test]
    fn balanceof_precompile() {
        let caller = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");
        let caller_balance = U256::from(10);

        let db = InMemoryDB::default();
        let mut evm = Evm::builder()
            .with_db(db)
            .modify_db(|db| {
                let caller_info = AccountInfo {
                    balances: HashMap::from([(BASE_TOKEN_ID, caller_balance)]),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(caller, caller_info);
            })
            .modify_tx_env(|tx| {
                tx.caller = caller;
                tx.transact_to = TransactTo::Call(NATIVE_TOKENS_PRECOMPILE_ADDRESS);

                // Compose the Tx Data, as follows: the BALANCEOF id + token_id + address
                const BALANCEOF_ID: u8 = 0xC0;
                let mut concatenated = vec![BALANCEOF_ID];
                concatenated.append(BASE_TOKEN_ID.to_be_bytes_vec().as_mut());
                concatenated.append(caller.to_vec().as_mut());
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

    /// TODO: use a bytecode that includes the BURN opcode
    #[test]
    fn burn_opcode() {
        let caller_contract = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");
        let sub_id = U256::from(2);
        let minted_token_id = token_id_address(caller_contract, sub_id);
        let caller_initial_balance = U256::from(1000);

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                db.token_ids.push(minted_token_id);
                let caller_info = AccountInfo {
                    balances: HashMap::from([(minted_token_id, caller_initial_balance)]),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 1,
                };
                db.insert_account_info(caller_contract, caller_info);
            })
            .build();

        // Test the burning of the native token
        let amount_to_burn = U256::from(500);
        assert!(evm.context.burn(caller_contract, sub_id, amount_to_burn));

        let journaled_state = evm.context.evm.inner.journaled_state.clone();
        let caller_minted_token_balance = *journaled_state
            .account(caller_contract)
            .info
            .balances
            .get(&minted_token_id)
            .unwrap();

        let expected_remaining_balance = caller_initial_balance - amount_to_burn;
        assert_eq!(caller_minted_token_balance, expected_remaining_balance);
    }

    /// TODO: add EOA check in precompile and route call via SRF-20
    #[test]
    fn burn_precompile() {
        let caller_contract = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");
        let sub_id = U256::from(2);
        let minted_token_id = token_id_address(caller_contract, sub_id);
        let caller_initial_balance = U256::from(1000);
        let amount_to_burn = U256::from(500);

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                db.token_ids.push(minted_token_id);
                let caller_info = AccountInfo {
                    balances: HashMap::from([(minted_token_id, caller_initial_balance)]),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 1,
                };
                db.insert_account_info(caller_contract, caller_info);
            })
            .modify_tx_env(|tx| {
                tx.caller = caller_contract;
                tx.transact_to = TransactTo::Call(NATIVE_TOKENS_PRECOMPILE_ADDRESS);

                // Compose the Tx Data, as follows: the BURN id + sub_id + amount
                const BURN_ID: u8 = 0xC2;
                let mut concatenated = vec![BURN_ID];
                concatenated.append(sub_id.to_be_bytes_vec().as_mut());
                concatenated.append(amount_to_burn.to_be_bytes_vec().as_mut());
                tx.data = Bytes::from(concatenated);
            })
            .build();

        let tx_result = evm.transact_commit();
        assert!(tx_result.is_ok());

        let execution_result = tx_result.unwrap();
        assert!(execution_result.is_success());

        let caller_minted_token_balance = evm
            .context
            .balance(minted_token_id, caller_contract)
            .unwrap()
            .0;
        assert_eq!(
            caller_minted_token_balance,
            caller_initial_balance - amount_to_burn
        );
    }

    /// TODO: use a bytecode that includes the MINT opcode
    #[test]
    fn mint_opcode() {
        let caller_contract = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                let caller_info = AccountInfo {
                    balances: HashMap::new(),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 1,
                };
                db.insert_account_info(caller_contract, caller_info);
            })
            .build();

        // Test the minting of a native token
        let sub_id = U256::from(2);
        let amount_to_mint = U256::from(1000);
        assert!(evm
            .context
            .mint(caller_contract, caller_contract, sub_id, amount_to_mint));

        let minted_token_id = token_id_address(caller_contract, sub_id);

        let journaled_state = evm.context.evm.inner.journaled_state.clone();
        let caller_minted_token_balance = *journaled_state
            .account(caller_contract)
            .info
            .balances
            .get(&minted_token_id)
            .unwrap();

        assert_eq!(caller_minted_token_balance, U256::from(1000));
    }

    /// TODO: add EOA check in precompile and route call via SRF-20
    #[test]
    fn mint_precompile() {
        let caller_contract = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");
        let sub_id = U256::from(2);
        let amount_to_mint = U256::from(1000);

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                let caller_info = AccountInfo {
                    balances: HashMap::new(),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 1,
                };
                db.insert_account_info(caller_contract, caller_info);
            })
            .modify_tx_env(|tx| {
                tx.caller = caller_contract; // tx.origin == msg.sender
                                             // EOA => SRF20 => Precompile
                tx.transact_to = TransactTo::Call(NATIVE_TOKENS_PRECOMPILE_ADDRESS);

                // Compose the Tx Data, as follows: the MINT id + recipient + sub_id + amount
                const MINT_ID: u8 = 0xC1;
                let recipient = caller_contract;

                let mut concatenated = vec![MINT_ID];
                concatenated.append(recipient.to_vec().as_mut());
                concatenated.append(sub_id.to_be_bytes_vec().as_mut());
                concatenated.append(amount_to_mint.to_be_bytes_vec().as_mut());
                tx.data = Bytes::from(concatenated);
            })
            .build();

        let tx_result = evm.transact_commit();
        assert!(tx_result.is_ok());

        let execution_result = tx_result.unwrap();
        assert!(execution_result.is_success());

        let minted_token_id = token_id_address(caller_contract, sub_id);
        let caller_minted_token_balance = evm
            .context
            .balance(minted_token_id, caller_contract)
            .unwrap()
            .0;
        assert_eq!(caller_minted_token_balance, amount_to_mint);
    }

    #[test]
    fn gas_calculation_underflow() {
        let callee = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");

        // https://github.com/bluealloy/revm/issues/277
        // checks this use case
        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                let code = bytes!("5b597fb075978b6c412c64d169d56d839a8fe01b3f4607ed603b2c78917ce8be1430fe6101e8527ffe64706ecad72a2f5c97a95e006e279dc57081902029ce96af7edae5de116fec610208527f9fc1ef09d4dd80683858ae3ea18869fe789ddc365d8d9d800e26c9872bac5e5b6102285260276102485360d461024953601661024a53600e61024b53607d61024c53600961024d53600b61024e5360b761024f5360596102505360796102515360a061025253607261025353603a6102545360fb61025553601261025653602861025753600761025853606f61025953601761025a53606161025b53606061025c5360a661025d53602b61025e53608961025f53607a61026053606461026153608c6102625360806102635360d56102645360826102655360ae61026653607f6101e8610146610220677a814b184591c555735fdcca53617f4d2b9134b29090c87d01058e27e962047654f259595947443b1b816b65cdb6277f4b59c10a36f4e7b8658f5a5e6f5561");
                let info = AccountInfo {
                    balances: HashMap::from([(BASE_TOKEN_ID, "0x100c5d668240db8e00".parse().unwrap())]),
                    code_hash: keccak256(&code),
                    code: Some(Bytecode::new_raw(code)),
                    nonce: 1,
                };
                db.insert_account_info(callee, info);
            })
            .modify_tx_env(|tx| {
                tx.caller = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");
                tx.transact_to = TransactTo::Call(callee);
                tx.data = Bytes::new();
                tx.transferred_tokens = vec![(TokenTransfer{ id: BASE_TOKEN_ID, amount: U256::ZERO})];
            })
            .with_external_context(CustomPrintTracer::default())
            .with_spec_id(SpecId::BERLIN)
            .append_handler_register(inspector_handle_register)
            .build();

        evm.transact().expect("Transaction to work");
    }

    #[test]
    fn transfer_base_token() {
        let callee_eoa = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");
        let caller_eoa = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");

        let mut db = InMemoryDB::default();
        let mut evm = Evm::builder()
            .with_db(db)
            .modify_db(|db| {
                let callee_info = AccountInfo {
                    balances: HashMap::from([(BASE_TOKEN_ID, U256::ZERO)]),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(callee_eoa, callee_info);

                let caller_info = AccountInfo {
                    balances: HashMap::from([(BASE_TOKEN_ID, U256::from(10))]),
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
                tx.transferred_tokens = vec![
                    (TokenTransfer {
                        id: BASE_TOKEN_ID,
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
            .get(&BASE_TOKEN_ID)
            .unwrap();
        assert_eq!(callee_base_balance, U256::from(10));
    }
}
