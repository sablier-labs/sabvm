//! Custom print inspector, it has step level information of execution.
//! It is a great tool if some debugging is needed.

use crate::{
    inspectors::GasInspector,
    interpreter::{CallInputs, CallOutcome, CreateInputs, CreateOutcome, Interpreter, OpCode},
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
            inputs.values,
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
            address, bytes, keccak256, token_id_address, AccountInfo, Address, Balances, Bytecode,
            Bytes, SpecId, TokenTransfer, TransactTo, B256, BASE_TOKEN_ID, U256,
        },
        sablier::native_tokens::{ADDRESS as NATIVE_TOKENS_PRECOMPILE_ADDRESS, BALANCEOF_SELECTOR},
        Evm, InMemoryDB,
    };
    use revm_interpreter::Host;
    use revm_precompile::HashMap;

    static NATIVE_TOKENS_LIBRARY_BYTECODE: Bytes = bytes!("7300000000000000000000000000000000000000003014608060405260043610610034575f3560e01c80633656eec214610038575b5f80fd5b610052600480360381019061004d9190610231565b610068565b60405161005f919061027e565b60405180910390f35b5f80838360405160240161007d9291906102b5565b604051602081830303815290604052633656eec260e01b6020820180517bffffffffffffffffffffffffffffffffffffffffffffffffffffffff838183161783525050505090505f8073706000000000000000000000000000000000000173ffffffffffffffffffffffffffffffffffffffff16836040516100ff9190610348565b5f60405180830381855afa9150503d805f8114610137576040519150601f19603f3d011682016040523d82523d5f602084013e61013c565b606091505b509150915081610181576040517f17e60c82000000000000000000000000000000000000000000000000000000008152600401610178906103b8565b60405180910390fd5b8080602001905181019061019591906103ea565b935050505092915050565b5f80fd5b5f819050919050565b6101b6816101a4565b81146101c0575f80fd5b50565b5f813590506101d1816101ad565b92915050565b5f73ffffffffffffffffffffffffffffffffffffffff82169050919050565b5f610200826101d7565b9050919050565b610210816101f6565b811461021a575f80fd5b50565b5f8135905061022b81610207565b92915050565b5f8060408385031215610247576102466101a0565b5b5f610254858286016101c3565b92505060206102658582860161021d565b9150509250929050565b610278816101a4565b82525050565b5f6020820190506102915f83018461026f565b92915050565b6102a0816101a4565b82525050565b6102af816101f6565b82525050565b5f6040820190506102c85f830185610297565b6102d560208301846102a6565b9392505050565b5f81519050919050565b5f81905092915050565b5f5b8381101561030d5780820151818401526020810190506102f2565b5f8484015250505050565b5f610322826102dc565b61032c81856102e6565b935061033c8185602086016102f0565b80840191505092915050565b5f6103538284610318565b915081905092915050565b5f82825260208201905092915050565b7f4e6174697665546f6b656e733a2062616c616e63654f66206661696c656400005f82015250565b5f6103a2601e8361035e565b91506103ad8261036e565b602082019050919050565b5f6020820190508181035f8301526103cf81610396565b9050919050565b5f815190506103e4816101ad565b92915050565b5f602082840312156103ff576103fe6101a0565b5b5f61040c848285016103d6565b9150509291505056fea164736f6c634300081a000a"); // The bytecode of the library serving as a proxy/adaptor between the Sablier Precompile and the SabVM smart contracts

    const NATIVE_TOKENS_LIBRARY_ADDRESS: Address =
        address!("5fdcca53617f4d2b9134b29090c87d01058e27e5"); // The address of the native_tokens library contract. Note: there's nothing special about this address. It's random, and is defined as a constant to make the tests more readable.

    static SRF20_MOCK_BYTECODE: Bytes = bytes!("608060405234801561000f575f80fd5b506004361061007b575f3560e01c806340c10f191161005957806340c10f19146100d957806342966c68146100f557806395d89b4114610111578063b3cea2171461012f5761007b565b806306fdde031461007f57806318160ddd1461009d578063313ce567146100bb575b5f80fd5b61008761014d565b604051610094919061067d565b60405180910390f35b6100a56101dc565b6040516100b291906106b5565b60405180910390f35b6100c36101e2565b6040516100d091906106e9565b60405180910390f35b6100f360048036038101906100ee919061078a565b6101ea565b005b61010f600480360381019061010a91906107c8565b6101f8565b005b610119610205565b604051610126919061067d565b60405180910390f35b610137610295565b60405161014491906106b5565b60405180910390f35b60605f805461015b90610820565b80601f016020809104026020016040519081016040528092919081815260200182805461018790610820565b80156101d25780601f106101a9576101008083540402835291602001916101d2565b820191905f5260205f20905b8154815290600101906020018083116101b557829003601f168201915b5050505050905090565b60025481565b5f6012905090565b6101f482826102d0565b5050565b6102023382610387565b50565b60606001805461021490610820565b80601f016020809104026020016040519081016040528092919081815260200182805461024090610820565b801561028b5780601f106102625761010080835404028352916020019161028b565b820191905f5260205f20905b81548152906001019060200180831161026e57829003601f168201915b5050505050905090565b5f80305f6040516020016102aa9291906108b5565b60405160208183030381529060405290505f81805190602001209050805f1c9250505090565b5f73ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff160361034057816040517f088b09aa00000000000000000000000000000000000000000000000000000000815260040161033791906108ef565b60405180910390fd5b61036b5f828473ffffffffffffffffffffffffffffffffffffffff166104359092919063ffffffff16565b8060025f82825461037c9190610935565b925050819055505050565b5f73ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff16036103f757816040517f1e4f3d3f0000000000000000000000000000000000000000000000000000000081526004016103ee91906108ef565b60405180910390fd5b6104225f828473ffffffffffffffffffffffffffffffffffffffff166105149092919063ffffffff16565b8060025f82825403925050819055505050565b5f82848360405160240161044b93929190610968565b60405160208183030381529060405263836a104060e01b6020820180517bffffffffffffffffffffffffffffffffffffffffffffffffffffffff838183161783525050505090505f73706000000000000000000000000000000000000173ffffffffffffffffffffffffffffffffffffffff16826040516104cc91906109e1565b5f60405180830381855af49150503d805f8114610504576040519150601f19603f3d011682016040523d82523d5f602084013e610509565b606091505b505090505050505050565b5f82848360405160240161052a93929190610968565b604051602081830303815290604052639eea5f6660e01b6020820180517bffffffffffffffffffffffffffffffffffffffffffffffffffffffff838183161783525050505090505f73706000000000000000000000000000000000000173ffffffffffffffffffffffffffffffffffffffff16826040516105ab91906109e1565b5f60405180830381855af49150503d805f81146105e3576040519150601f19603f3d011682016040523d82523d5f602084013e6105e8565b606091505b505090505050505050565b5f81519050919050565b5f82825260208201905092915050565b5f5b8381101561062a57808201518184015260208101905061060f565b5f8484015250505050565b5f601f19601f8301169050919050565b5f61064f826105f3565b61065981856105fd565b935061066981856020860161060d565b61067281610635565b840191505092915050565b5f6020820190508181035f8301526106958184610645565b905092915050565b5f819050919050565b6106af8161069d565b82525050565b5f6020820190506106c85f8301846106a6565b92915050565b5f60ff82169050919050565b6106e3816106ce565b82525050565b5f6020820190506106fc5f8301846106da565b92915050565b5f80fd5b5f73ffffffffffffffffffffffffffffffffffffffff82169050919050565b5f61072f82610706565b9050919050565b61073f81610725565b8114610749575f80fd5b50565b5f8135905061075a81610736565b92915050565b6107698161069d565b8114610773575f80fd5b50565b5f8135905061078481610760565b92915050565b5f80604083850312156107a05761079f610702565b5b5f6107ad8582860161074c565b92505060206107be85828601610776565b9150509250929050565b5f602082840312156107dd576107dc610702565b5b5f6107ea84828501610776565b91505092915050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52602260045260245ffd5b5f600282049050600182168061083757607f821691505b60208210810361084a576108496107f3565b5b50919050565b5f8160601b9050919050565b5f61086682610850565b9050919050565b5f6108778261085c565b9050919050565b61088f61088a82610725565b61086d565b82525050565b5f819050919050565b6108af6108aa8261069d565b610895565b82525050565b5f6108c0828561087e565b6014820191506108d0828461089e565b6020820191508190509392505050565b6108e981610725565b82525050565b5f6020820190506109025f8301846108e0565b92915050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52601160045260245ffd5b5f61093f8261069d565b915061094a8361069d565b925082820190508082111561096257610961610908565b5b92915050565b5f60608201905061097b5f8301866106a6565b61098860208301856108e0565b61099560408301846106a6565b949350505050565b5f81519050919050565b5f81905092915050565b5f6109bb8261099d565b6109c581856109a7565b93506109d581856020860161060d565b80840191505092915050565b5f6109ec82846109b1565b91508190509291505056fea164736f6c634300081a000a");

    const SRF20_MOCK_ADDRESS: Address = address!("5fdcca53617f4d2b9134b29090c87d01058e27e6"); // The address of the SRF20 Mock. Note: there's nothing special about this address. It's random, and is defined as a constant to make the tests more readable.

    static NAIVE_TOKEN_TRANSFERRER_MOCK_BYTECODE: Bytes = bytes!("608060405234801561000f575f80fd5b506004361061004a575f3560e01c8063095bcdb61461004e578063822bbe4c1461006a5780639958341714610086578063d1c673e9146100a2575b5f80fd5b610068600480360381019061006391906105bf565b6100be565b005b610084600480360381019061007f91906106c5565b6100ee565b005b6100a0600480360381019061009b9190610789565b61012a565b005b6100bc60048036038101906100b7919061081a565b610160565b005b6100e982828573ffffffffffffffffffffffffffffffffffffffff166101969092919063ffffffff16565b505050565b6101218686868686868d73ffffffffffffffffffffffffffffffffffffffff16610275909695949392919063ffffffff16565b50505050505050565b610159848484848973ffffffffffffffffffffffffffffffffffffffff1661036090949392919063ffffffff16565b5050505050565b61018f848484848973ffffffffffffffffffffffffffffffffffffffff1661044590949392919063ffffffff16565b5050505050565b5f8383836040516024016101ac939291906108bc565b60405160208183030381529060405263095bcdb660e01b6020820180517bffffffffffffffffffffffffffffffffffffffffffffffffffffffff838183161783525050505090505f73706000000000000000000000000000000000000173ffffffffffffffffffffffffffffffffffffffff168260405161022d919061095d565b5f60405180830381855af49150503d805f8114610265576040519150601f19603f3d011682016040523d82523d5f602084013e61026a565b606091505b505090505050505050565b5f878787878787876040516024016102939796959493929190610a45565b60405160208183030381529060405263822bbe4c60e01b6020820180517bffffffffffffffffffffffffffffffffffffffffffffffffffffffff838183161783525050505090505f73706000000000000000000000000000000000000173ffffffffffffffffffffffffffffffffffffffff1682604051610314919061095d565b5f60405180830381855af49150503d805f811461034c576040519150601f19603f3d011682016040523d82523d5f602084013e610351565b606091505b50509050505050505050505050565b5f858585858560405160240161037a959493929190610aa3565b604051602081830303815290604052639958341760e01b6020820180517bffffffffffffffffffffffffffffffffffffffffffffffffffffffff838183161783525050505090505f73706000000000000000000000000000000000000173ffffffffffffffffffffffffffffffffffffffff16826040516103fb919061095d565b5f60405180830381855af49150503d805f8114610433576040519150601f19603f3d011682016040523d82523d5f602084013e610438565b606091505b5050905050505050505050565b5f858585858560405160240161045f959493929190610aea565b60405160208183030381529060405263d1c673e960e01b6020820180517bffffffffffffffffffffffffffffffffffffffffffffffffffffffff838183161783525050505090505f73706000000000000000000000000000000000000173ffffffffffffffffffffffffffffffffffffffff16826040516104e0919061095d565b5f60405180830381855af49150503d805f8114610518576040519150601f19603f3d011682016040523d82523d5f602084013e61051d565b606091505b5050905050505050505050565b5f80fd5b5f80fd5b5f73ffffffffffffffffffffffffffffffffffffffff82169050919050565b5f61055b82610532565b9050919050565b61056b81610551565b8114610575575f80fd5b50565b5f8135905061058681610562565b92915050565b5f819050919050565b61059e8161058c565b81146105a8575f80fd5b50565b5f813590506105b981610595565b92915050565b5f805f606084860312156105d6576105d561052a565b5b5f6105e386828701610578565b93505060206105f4868287016105ab565b9250506040610605868287016105ab565b9150509250925092565b5f80fd5b5f80fd5b5f80fd5b5f8083601f8401126106305761062f61060f565b5b8235905067ffffffffffffffff81111561064d5761064c610613565b5b60208301915083602082028301111561066957610668610617565b5b9250929050565b5f8083601f8401126106855761068461060f565b5b8235905067ffffffffffffffff8111156106a2576106a1610613565b5b6020830191508360018202830111156106be576106bd610617565b5b9250929050565b5f805f805f805f6080888a0312156106e0576106df61052a565b5b5f6106ed8a828b01610578565b975050602088013567ffffffffffffffff81111561070e5761070d61052e565b5b61071a8a828b0161061b565b9650965050604088013567ffffffffffffffff81111561073d5761073c61052e565b5b6107498a828b0161061b565b9450945050606088013567ffffffffffffffff81111561076c5761076b61052e565b5b6107788a828b01610670565b925092505092959891949750929550565b5f805f805f606086880312156107a2576107a161052a565b5b5f6107af88828901610578565b955050602086013567ffffffffffffffff8111156107d0576107cf61052e565b5b6107dc8882890161061b565b9450945050604086013567ffffffffffffffff8111156107ff576107fe61052e565b5b61080b8882890161061b565b92509250509295509295909350565b5f805f805f608086880312156108335761083261052a565b5b5f61084088828901610578565b9550506020610851888289016105ab565b9450506040610862888289016105ab565b935050606086013567ffffffffffffffff8111156108835761088261052e565b5b61088f88828901610670565b92509250509295509295909350565b6108a781610551565b82525050565b6108b68161058c565b82525050565b5f6060820190506108cf5f83018661089e565b6108dc60208301856108ad565b6108e960408301846108ad565b949350505050565b5f81519050919050565b5f81905092915050565b5f5b83811015610922578082015181840152602081019050610907565b5f8484015250505050565b5f610937826108f1565b61094181856108fb565b9350610951818560208601610905565b80840191505092915050565b5f610968828461092d565b915081905092915050565b5f82825260208201905092915050565b5f80fd5b82818337505050565b5f61099b8385610973565b93507f07ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff8311156109ce576109cd610983565b5b6020830292506109df838584610987565b82840190509392505050565b5f82825260208201905092915050565b828183375f83830152505050565b5f601f19601f8301169050919050565b5f610a2483856109eb565b9350610a318385846109fb565b610a3a83610a09565b840190509392505050565b5f608082019050610a585f83018a61089e565b8181036020830152610a6b81888a610990565b90508181036040830152610a80818688610990565b90508181036060830152610a95818486610a19565b905098975050505050505050565b5f606082019050610ab65f83018861089e565b8181036020830152610ac9818688610990565b90508181036040830152610ade818486610990565b90509695505050505050565b5f608082019050610afd5f83018861089e565b610b0a60208301876108ad565b610b1760408301866108ad565b8181036060830152610b2a818486610a19565b9050969550505050505056fea164736f6c634300081a000a");

    const NAIVE_TOKEN_TRANSFERRER_MOCK_ADDRESS: Address =
        address!("5fdcca53617f4d2b9134b29090c87d01058e27e8"); // The address of the Mock Contract to Transfer and Call. Note: there's nothing special about this address. It's random, and is defined as a constant to make the tests more readable.

    static CONTRACT_TO_TRANSFER_AND_CALL_TO_BYTECODE: Bytes = bytes!("608060405260043610610028575f3560e01c806365066c971461002c578063ffb4fc7514610048575b5f80fd5b610046600480360381019061004191906103e5565b610064565b005b610062600480360381019061005d9190610488565b610192565b005b8282905085859050146100ac576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004016100a39061056c565b60405180910390fd5b5f5b85859050811015610189578383828181106100cc576100cb61058a565b5b905060200201358210610114576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040161010b90610627565b60405180910390fd5b61017c86868381811061012a5761012961058a565b5b90506020020135838686858181106101455761014461058a565b5b905060200201356101569190610672565b8973ffffffffffffffffffffffffffffffffffffffff166102109092919063ffffffff16565b80806001019150506100ae565b50505050505050565b8181106101d4576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004016101cb90610627565b60405180910390fd5b61020a8382846101e49190610672565b8673ffffffffffffffffffffffffffffffffffffffff166102109092919063ffffffff16565b50505050565b5f838383604051602401610226939291906106c3565b60405160208183030381529060405263095bcdb660e01b6020820180517bffffffffffffffffffffffffffffffffffffffffffffffffffffffff838183161783525050505090505f73706000000000000000000000000000000000000173ffffffffffffffffffffffffffffffffffffffff16826040516102a79190610764565b5f60405180830381855af49150503d805f81146102df576040519150601f19603f3d011682016040523d82523d5f602084013e6102e4565b606091505b505090505050505050565b5f80fd5b5f80fd5b5f73ffffffffffffffffffffffffffffffffffffffff82169050919050565b5f610320826102f7565b9050919050565b61033081610316565b811461033a575f80fd5b50565b5f8135905061034b81610327565b92915050565b5f80fd5b5f80fd5b5f80fd5b5f8083601f84011261037257610371610351565b5b8235905067ffffffffffffffff81111561038f5761038e610355565b5b6020830191508360208202830111156103ab576103aa610359565b5b9250929050565b5f819050919050565b6103c4816103b2565b81146103ce575f80fd5b50565b5f813590506103df816103bb565b92915050565b5f805f805f80608087890312156103ff576103fe6102ef565b5b5f61040c89828a0161033d565b965050602087013567ffffffffffffffff81111561042d5761042c6102f3565b5b61043989828a0161035d565b9550955050604087013567ffffffffffffffff81111561045c5761045b6102f3565b5b61046889828a0161035d565b9350935050606061047b89828a016103d1565b9150509295509295509295565b5f805f80608085870312156104a05761049f6102ef565b5b5f6104ad8782880161033d565b94505060206104be878288016103d1565b93505060406104cf878288016103d1565b92505060606104e0878288016103d1565b91505092959194509250565b5f82825260208201905092915050565b7f546f6b656e2049447320616e6420616d6f756e7473206d7573742068617665205f8201527f7468652073616d65206c656e6774680000000000000000000000000000000000602082015250565b5f610556602f836104ec565b9150610561826104fc565b604082019050919050565b5f6020820190508181035f8301526105838161054a565b9050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52603260045260245ffd5b7f466565206d757374206265206c657373207468616e2074686520616d6f756e745f8201527f20746f207472616e736665720000000000000000000000000000000000000000602082015250565b5f610611602c836104ec565b915061061c826105b7565b604082019050919050565b5f6020820190508181035f83015261063e81610605565b9050919050565b7f4e487b71000000000000000000000000000000000000000000000000000000005f52601160045260245ffd5b5f61067c826103b2565b9150610687836103b2565b925082820390508181111561069f5761069e610645565b5b92915050565b6106ae81610316565b82525050565b6106bd816103b2565b82525050565b5f6060820190506106d65f8301866106a5565b6106e360208301856106b4565b6106f060408301846106b4565b949350505050565b5f81519050919050565b5f81905092915050565b5f5b8381101561072957808201518184015260208101905061070e565b5f8484015250505050565b5f61073e826106f8565b6107488185610702565b935061075881856020860161070c565b80840191505092915050565b5f61076f8284610734565b91508190509291505056fea164736f6c634300081a000a");

    const CONTRACT_TO_TRANSFER_AND_CALL_TO_ADDRESS: Address =
        address!("5fdcca53617f4d2b9134b29090c87d01058e27e3"); // The address of the Contract to Transfer and Call to. Note: there's nothing special about this address. It's random, and is defined as a constant to make the tests more readable.

    #[test]
    fn balanceof_precompile() {
        let caller = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");
        let caller_balance = U256::from(10);

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
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

                // Compose the Tx Data, as follows: the balanceOf() function selector + token_id + address
                let mut concatenated = BALANCEOF_SELECTOR.to_be_bytes().to_vec();
                concatenated.append(BASE_TOKEN_ID.to_be_bytes_vec().as_mut());
                let caller_address_evm_word = caller.into_word();
                concatenated.append(caller_address_evm_word.to_vec().as_mut());
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
    fn balanceof_precompile_native_tokens_library() {
        let caller = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");
        let caller_balance = U256::from(11);

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                let caller_info = AccountInfo {
                    balances: HashMap::from([(BASE_TOKEN_ID, caller_balance)]),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(caller, caller_info);

                let library_bytecode = &NATIVE_TOKENS_LIBRARY_BYTECODE;

                let info = AccountInfo {
                    balances: HashMap::new(),
                    code_hash: keccak256(library_bytecode.clone()),
                    code: Some(Bytecode::new_raw(library_bytecode.clone())),
                    nonce: 1,
                };
                db.insert_account_info(NATIVE_TOKENS_LIBRARY_ADDRESS, info);
            })
            .modify_tx_env(|tx| {
                tx.caller = caller;
                tx.transact_to = TransactTo::Call(NATIVE_TOKENS_LIBRARY_ADDRESS);

                // Compose the Tx Data, as follows: the balanceOf() function selector + token_id + address
                let mut concatenated = BALANCEOF_SELECTOR.to_be_bytes().to_vec();
                concatenated.append(BASE_TOKEN_ID.to_be_bytes_vec().as_mut());
                let caller_address_evm_word = caller.into_word();
                concatenated.append(caller_address_evm_word.to_vec().as_mut());
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
    fn mint_precompile_native_tokens_library_srf20mock() {
        let caller = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");
        let amount_to_mint = U256::from(1000);

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                let caller_info = AccountInfo {
                    balances: HashMap::new(),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(caller, caller_info);

                let srf20_mock_bytecode = &SRF20_MOCK_BYTECODE;
                let callee_info = AccountInfo {
                    balances: HashMap::new(),
                    code_hash: keccak256(srf20_mock_bytecode.clone()),
                    code: Some(Bytecode::new_raw(srf20_mock_bytecode.clone())),
                    nonce: 1,
                };
                db.insert_account_info(SRF20_MOCK_ADDRESS, callee_info);
            })
            .modify_tx_env(|tx| {
                tx.caller = caller;
                tx.transact_to = TransactTo::Call(SRF20_MOCK_ADDRESS);

                // Compose the Tx Data
                let mut concatenated = bytes!("40c10f19").to_vec(); // the selector of "mint(address, uint256)"
                let recipient_address_evm_word = tx.caller.into_word();
                concatenated.append(recipient_address_evm_word.to_vec().as_mut());
                concatenated.append(amount_to_mint.to_be_bytes_vec().as_mut());
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

        let minted_token_id = token_id_address(SRF20_MOCK_ADDRESS, U256::ZERO);
        let caller_minted_token_balance = evm.context.balance(minted_token_id, caller).unwrap().0;
        assert_eq!(caller_minted_token_balance, amount_to_mint);
    }

    #[test]
    fn burn_precompile_native_tokens_library_srf20mock() {
        let caller = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");
        let sub_id = U256::from(0);
        let minted_token_id = token_id_address(SRF20_MOCK_ADDRESS, sub_id);
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
                    nonce: 0,
                };
                db.insert_account_info(caller, caller_info);

                let srf20_mock_bytecode = &SRF20_MOCK_BYTECODE;
                let callee_info = AccountInfo {
                    balances: HashMap::new(),
                    code_hash: keccak256(srf20_mock_bytecode.clone()),
                    code: Some(Bytecode::new_raw(srf20_mock_bytecode.clone())),
                    nonce: 1,
                };
                db.insert_account_info(SRF20_MOCK_ADDRESS, callee_info);
            })
            .modify_tx_env(|tx| {
                tx.caller = caller;
                tx.transact_to = TransactTo::Call(SRF20_MOCK_ADDRESS);

                // Compose the Tx Data
                let mut concatenated = bytes!("42966c68").to_vec(); // the selector of "burn(uint256)"
                concatenated.append(amount_to_burn.to_be_bytes_vec().as_mut());
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

        let caller_token_balance = evm.context.balance(minted_token_id, caller).unwrap().0;
        assert_eq!(
            caller_token_balance,
            caller_initial_balance - amount_to_burn
        );
    }

    #[test]
    fn mntcallvalues_precompile() {
        use crate::primitives::{
            address, token_id_address, utilities::bytes_parsing::*, AccountInfo, Bytes,
            TokenTransfer, TransactTo, B256, BASE_TOKEN_ID, U256,
        };

        let caller = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");
        let caller_token1_balance = U256::from(10);

        let sub_id2 = U256::from(2);
        let token2_id = token_id_address(NATIVE_TOKENS_PRECOMPILE_ADDRESS, sub_id2);
        let caller_token2_balance = U256::from(100);

        let sub_id3 = U256::from(3);
        let token3_id = token_id_address(NATIVE_TOKENS_PRECOMPILE_ADDRESS, sub_id3);
        let caller_token3_balance = U256::from(1000);

        let tokens_to_be_transferred = vec![
            TokenTransfer {
                id: BASE_TOKEN_ID,
                amount: caller_token1_balance,
            },
            TokenTransfer {
                id: token2_id,
                amount: caller_token2_balance,
            },
            TokenTransfer {
                id: token3_id,
                amount: caller_token3_balance,
            },
        ];

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                db.token_ids.extend(vec![token2_id, token3_id]);

                let callee_info = AccountInfo {
                    balances: HashMap::default(),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(NATIVE_TOKENS_PRECOMPILE_ADDRESS, callee_info);

                let caller_info = AccountInfo {
                    balances: HashMap::from([
                        (BASE_TOKEN_ID, caller_token1_balance),
                        (token2_id, caller_token2_balance),
                        (token3_id, caller_token3_balance),
                    ]),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(caller, caller_info);
            })
            .modify_tx_env(|tx| {
                tx.caller = caller;
                tx.transact_to = TransactTo::Call(NATIVE_TOKENS_PRECOMPILE_ADDRESS);

                //Compose the Tx Data, as follows: the MNTCALLVALUES id + address + token_id
                const MNTCALLVALUES_ID: u32 = 0x2F;
                tx.data = Bytes::from(MNTCALLVALUES_ID.to_be_bytes());
                tx.transferred_tokens.clone_from(&tokens_to_be_transferred);
            })
            .with_external_context(CustomPrintTracer::default())
            .with_spec_id(SpecId::LATEST)
            .append_handler_register(inspector_handle_register)
            .build();

        let tx_result = evm.transact_commit();
        assert!(tx_result.is_ok());

        let execution_result = tx_result.unwrap();
        assert!(execution_result.is_success());

        let mut tx_output = Bytes::copy_from_slice(execution_result.output().unwrap());
        let transferred_tokens_no = match consume_usize_from(&mut tx_output) {
            Ok(value) => value,
            Err(_) => panic!("Failed to consume usize from output"),
        };

        assert_eq!(transferred_tokens_no, 3);

        let mut transferred_tokens = Vec::new();

        for _ in 0..transferred_tokens_no {
            let token_id = match consume_u256_from(&mut tx_output) {
                Ok(value) => value,
                Err(_) => panic!("Failed to consume token id from output"),
            };

            let token_amount = match consume_u256_from(&mut tx_output) {
                Ok(value) => value,
                Err(_) => panic!("Failed to consume token amount from output"),
            };

            transferred_tokens.push(TokenTransfer {
                id: token_id,
                amount: token_amount,
            });
        }

        let mut sorted_vec1 = tokens_to_be_transferred.clone();
        sorted_vec1.sort();
        let mut sorted_vec2 = transferred_tokens.clone();
        sorted_vec2.sort();

        assert_eq!(sorted_vec1, sorted_vec2);
    }

    #[test]
    fn token_transfer_via_tx_eoa_to_eoa() {
        let callee_eoa = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");
        let caller_eoa = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                let callee_info = AccountInfo {
                    balances: HashMap::default(),
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
            .with_spec_id(SpecId::LATEST)
            .append_handler_register(inspector_handle_register)
            .build();

        let tx_result = evm.transact_commit();
        assert!(tx_result.is_ok());

        let execution_result = tx_result.unwrap();
        assert!(execution_result.is_success());

        let callee_base_balance = evm.context.balance(BASE_TOKEN_ID, callee_eoa).unwrap().0;
        assert_eq!(callee_base_balance, U256::from(10));
    }

    #[test]
    fn token_transfer_via_tx_eoa_to_contract() {
        let empty_contract_with_payable_external_fallback_bytecode: Bytes = bytes!("608060405200fea2646970667358221220b70791be49b3a1d958db814a6c76821c20ff6d9e801a0ac110775492d67abbba64736f6c634300081a0033"); // The bytecode of a contract with just an empty payable fallback function defined

        let callee = address!("5fdcca53617f4d2b9134b29090c87d01058e27e9");
        let caller_eoa = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                let info = AccountInfo {
                    balances: HashMap::default(),
                    code_hash: keccak256(
                        empty_contract_with_payable_external_fallback_bytecode.clone(),
                    ),
                    code: Some(Bytecode::new_raw(
                        empty_contract_with_payable_external_fallback_bytecode.clone(),
                    )),
                    nonce: 1,
                };

                db.insert_account_info(callee, info);

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
                tx.transact_to = TransactTo::Call(callee);
                tx.data = Bytes::new();
                tx.transferred_tokens = vec![
                    (TokenTransfer {
                        id: BASE_TOKEN_ID,
                        amount: U256::from(10),
                    }),
                ];
            })
            .with_external_context(CustomPrintTracer::default())
            .with_spec_id(SpecId::LATEST)
            .append_handler_register(inspector_handle_register)
            .build();

        let tx_result = evm.transact_commit();
        assert!(tx_result.is_ok());

        let execution_result = tx_result.unwrap();
        assert!(execution_result.is_success());

        let callee_base_balance = evm.context.balance(BASE_TOKEN_ID, callee).unwrap().0;
        assert_eq!(callee_base_balance, U256::from(10));
    }

    #[test]
    fn token_transfer_via_precompile() {
        let caller_eoa = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");
        let token_transferrer_balance = U256::from(10);
        let transfer_amount = U256::from(4);
        let token_id = U256::from(5); // Random token id

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                db.token_ids.push(token_id);

                let caller_info = AccountInfo {
                    balances: HashMap::default(),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(caller_eoa, caller_info);

                let token_transferrer_mock_bytecode = &NAIVE_TOKEN_TRANSFERRER_MOCK_BYTECODE;
                let callee_info = AccountInfo {
                    balances: HashMap::from([(token_id, token_transferrer_balance)]),
                    code_hash: keccak256(token_transferrer_mock_bytecode.clone()),
                    code: Some(Bytecode::new_raw(token_transferrer_mock_bytecode.clone())),
                    nonce: 1,
                };
                db.insert_account_info(NAIVE_TOKEN_TRANSFERRER_MOCK_ADDRESS, callee_info);
            })
            .modify_tx_env(|tx| {
                tx.caller = caller_eoa;
                tx.transact_to = TransactTo::Call(NAIVE_TOKEN_TRANSFERRER_MOCK_ADDRESS);

                // Compose the Tx Data
                let mut concatenated = bytes!("095bcdb6").to_vec(); // the selector of "transfer(address recipient, uint256 tokenID, uint256 amount)"
                let recipient_address_evm_word = tx.caller.into_word();
                concatenated.append(recipient_address_evm_word.to_vec().as_mut());
                concatenated.append(token_id.to_be_bytes_vec().as_mut());
                concatenated.append(transfer_amount.to_be_bytes_vec().as_mut());

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

        let caller_token_balance = evm.context.balance(token_id, caller_eoa).unwrap().0;
        assert_eq!(caller_token_balance, transfer_amount);
    }

    #[test]
    fn token_transfer_multiple_via_precompile() {
        let caller_eoa = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");

        let token_ids = vec![U256::from(5), U256::from(6)]; // Random token ids
        let token_transferrer_balances = [U256::from(10), U256::from(10)];
        let transfer_amounts = [U256::from(4), U256::from(6)];

        assert_eq!(token_ids.len(), token_transferrer_balances.len());
        assert_eq!(token_transferrer_balances.len(), transfer_amounts.len());

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                db.token_ids.append(&mut token_ids.clone());

                let caller_info = AccountInfo {
                    balances: HashMap::default(),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(caller_eoa, caller_info);

                let token_transferrer_mock_bytecode = &NAIVE_TOKEN_TRANSFERRER_MOCK_BYTECODE;
                let mut balances: Balances = HashMap::new();
                for (token_id, balance) in token_ids
                    .iter()
                    .zip(token_transferrer_balances.iter())
                    .collect::<Vec<(&U256, &U256)>>()
                {
                    balances.insert(*token_id, *balance);
                }
                let callee_info = AccountInfo {
                    balances,
                    code_hash: keccak256(token_transferrer_mock_bytecode.clone()),
                    code: Some(Bytecode::new_raw(token_transferrer_mock_bytecode.clone())),
                    nonce: 1,
                };
                db.insert_account_info(NAIVE_TOKEN_TRANSFERRER_MOCK_ADDRESS, callee_info);
            })
            .modify_tx_env(|tx| {
                tx.caller = caller_eoa;
                tx.transact_to = TransactTo::Call(NAIVE_TOKEN_TRANSFERRER_MOCK_ADDRESS);

                // Compose the Tx Data
                // tx.data structure:
                // 0 - recipient's address
                // 1/32 - token_ids_offset
                // 2/64 - amounts_offset
                // 3/96 - token_ids_len
                // 4/128 - token_ids
                // ~/~ - transfer_amounts_len
                // ~/~ - transfer_amounts

                let mut concatenated = bytes!("99583417").to_vec(); // the selector of "transferMultiple(address to, uint256[] calldata tokenIDs, uint256[] calldata amounts)"

                let recipient_address_evm_word = tx.caller.into_word();
                concatenated.append(recipient_address_evm_word.to_vec().as_mut());

                let token_ids_offset = U256::from(96);
                concatenated.append(token_ids_offset.to_be_bytes_vec().as_mut());

                let token_ids_len = U256::from_be_slice(token_ids.len().to_be_bytes().as_slice());

                let evm_word_size = U256::from(32);
                let amounts_offset =
                    token_ids_offset + ((U256::from(1) + token_ids_len) * evm_word_size);
                concatenated.append(amounts_offset.to_be_bytes_vec().as_mut());

                concatenated.append(token_ids_len.to_be_bytes_vec().as_mut());
                for token_id in token_ids.iter() {
                    concatenated.append(token_id.to_be_bytes_vec().as_mut());
                }

                let transfer_amounts_len =
                    U256::from_be_slice(transfer_amounts.len().to_be_bytes().as_slice());
                concatenated.append(transfer_amounts_len.to_be_bytes_vec().as_mut());
                for transfer_amount in transfer_amounts.iter() {
                    concatenated.append(transfer_amount.to_be_bytes_vec().as_mut());
                }

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

        for (token_id, transfer_amount) in token_ids.iter().zip(transfer_amounts.iter()) {
            let caller_token_balance = evm.context.balance(*token_id, caller_eoa).unwrap().0;
            assert_eq!(caller_token_balance, *transfer_amount);
        }
    }

    #[test]
    fn token_transfer_and_call_via_precompile() {
        let caller_eoa = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");
        let end_recipient_eoa = address!("5fdcca53617f4d2b9134b29090c87d01058e27a5");
        let token_transferrer_balance = U256::from(10);
        let transfer_amount = U256::from(4);
        let fee_amount = U256::from(1);
        let token_id = U256::from(5); // Random id

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                db.token_ids.push(token_id);

                let caller_info = AccountInfo {
                    balances: HashMap::from([(token_id, token_transferrer_balance + fee_amount)]),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(caller_eoa, caller_info);

                let token_transferrer_mock_bytecode = &NAIVE_TOKEN_TRANSFERRER_MOCK_BYTECODE;
                let token_transferrer_info = AccountInfo {
                    balances: HashMap::default(),
                    code_hash: keccak256(token_transferrer_mock_bytecode.clone()),
                    code: Some(Bytecode::new_raw(token_transferrer_mock_bytecode.clone())),
                    nonce: 1,
                };
                db.insert_account_info(
                    NAIVE_TOKEN_TRANSFERRER_MOCK_ADDRESS,
                    token_transferrer_info,
                );

                let callee_and_recipient_bytecode = &CONTRACT_TO_TRANSFER_AND_CALL_TO_BYTECODE;
                let callee_and_recipient_info = AccountInfo {
                    balances: HashMap::default(),
                    code_hash: keccak256(callee_and_recipient_bytecode.clone()),
                    code: Some(Bytecode::new_raw(callee_and_recipient_bytecode.clone())),
                    nonce: 1,
                };
                db.insert_account_info(
                    CONTRACT_TO_TRANSFER_AND_CALL_TO_ADDRESS,
                    callee_and_recipient_info,
                );
            })
            .modify_tx_env(|tx| {
                tx.caller = caller_eoa;
                tx.transact_to = TransactTo::Call(NAIVE_TOKEN_TRANSFERRER_MOCK_ADDRESS);

                // tx.data structure:
                // 0 - recipient-and-callee's address
                // 1/32 - token id
                // 2/64 - transfer amount
                // 3/96 - calldata offset (== 128)
                // 4/128 - calldata byte length
                // ~/~ - calldata bytes

                // Compose the Tx Data
                let mut concatenated = bytes!("d1c673e9").to_vec(); // the selector of "transferAndCall(address recipientAndCallee, uint256 tokenID, uint256 amount, bytes calldata data)"
                let recipient_and_callee_address_evm_word =
                    CONTRACT_TO_TRANSFER_AND_CALL_TO_ADDRESS.into_word();
                concatenated.append(recipient_and_callee_address_evm_word.to_vec().as_mut());
                concatenated.append(token_id.to_be_bytes_vec().as_mut());
                concatenated.append(transfer_amount.to_be_bytes_vec().as_mut());

                let calldata_offset = U256::from(128);
                concatenated.append(calldata_offset.to_be_bytes_vec().as_mut());

                let mut calldata_concatenated: Vec<u8> = Vec::new();
                let mut word = [0; 32];

                word[28..].copy_from_slice(bytes!("ffb4fc75").to_vec().as_slice()); // the selector of "transferTokenForAFee(address recipient, uint256 tokenID, uint256 amount, uint256 fee)"
                calldata_concatenated.append(word.to_vec().as_mut());

                let recipient_address_evm_word = end_recipient_eoa.into_word();
                calldata_concatenated.append(recipient_address_evm_word.to_vec().as_mut());
                calldata_concatenated.append(token_id.to_be_bytes_vec().as_mut());
                calldata_concatenated.append(transfer_amount.to_be_bytes_vec().as_mut());
                calldata_concatenated.append(fee_amount.to_be_bytes_vec().as_mut());

                let calldata_byte_length = U256::from(calldata_concatenated.len());
                concatenated.append(calldata_byte_length.to_be_bytes_vec().as_mut());
                concatenated.append(calldata_concatenated.as_mut());

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

        let recipient_token_balance = evm.context.balance(token_id, end_recipient_eoa).unwrap().0;
        assert_eq!(recipient_token_balance, transfer_amount - fee_amount);

        let mock_contract_token_balance = evm
            .context
            .balance(token_id, CONTRACT_TO_TRANSFER_AND_CALL_TO_ADDRESS)
            .unwrap()
            .0;
        assert_eq!(mock_contract_token_balance, fee_amount);
    }

    #[test]
    fn token_transfer_multiple_and_call_via_precompile() {
        let caller_eoa = address!("5fdcca53617f4d2b9134b29090c87d01058e27e0");
        let end_recipient_eoa = address!("5fdcca53617f4d2b9134b29090c87d01058e27a5");
        let token_transferrer_balance = U256::from(10);
        let transfer_amount = U256::from(4);
        let fee_amount = U256::from(1);
        let token1_id = U256::from(5); // Random id
        let token2_id = U256::from(6); // Random id

        let mut evm = Evm::builder()
            .with_db(InMemoryDB::default())
            .modify_db(|db| {
                db.token_ids.push(token1_id);

                let caller_info = AccountInfo {
                    balances: HashMap::from([
                        (token1_id, token_transferrer_balance + fee_amount),
                        (token2_id, token_transferrer_balance + fee_amount),
                    ]),
                    code_hash: B256::default(),
                    code: None,
                    nonce: 0,
                };
                db.insert_account_info(caller_eoa, caller_info);

                let token_transferrer_mock_bytecode = &NAIVE_TOKEN_TRANSFERRER_MOCK_BYTECODE;
                let token_transferrer_info = AccountInfo {
                    balances: HashMap::default(),
                    code_hash: keccak256(token_transferrer_mock_bytecode.clone()),
                    code: Some(Bytecode::new_raw(token_transferrer_mock_bytecode.clone())),
                    nonce: 1,
                };
                db.insert_account_info(
                    NAIVE_TOKEN_TRANSFERRER_MOCK_ADDRESS,
                    token_transferrer_info,
                );

                let callee_and_recipient_bytecode = &CONTRACT_TO_TRANSFER_AND_CALL_TO_BYTECODE;
                let callee_and_recipient_info = AccountInfo {
                    balances: HashMap::default(),
                    code_hash: keccak256(callee_and_recipient_bytecode.clone()),
                    code: Some(Bytecode::new_raw(callee_and_recipient_bytecode.clone())),
                    nonce: 1,
                };
                db.insert_account_info(
                    CONTRACT_TO_TRANSFER_AND_CALL_TO_ADDRESS,
                    callee_and_recipient_info,
                );
            })
            .modify_tx_env(|tx| {
                tx.caller = caller_eoa;
                tx.transact_to = TransactTo::Call(NAIVE_TOKEN_TRANSFERRER_MOCK_ADDRESS);

                // tx.data structure:
                // 0 - recipient-and-callee's address
                // 1/32 - token ids offset (== 128)
                // 2/64 - transfer amounts offset (== 224)
                // 3/96 - calldata offset (== 320)
                // 4/128 - token ids len
                // 5/160 - token ids::token1_id
                // 6/192 - token ids::token2_id
                // 7/224 - transfer amounts len
                // 8/256 - transfer amounts::transfer_amount (1)
                // 9/288 - transfer amounts::transfer_amount (2)
                // 10/320 - calldata byte length
                // ~/~ - calldata bytes

                // Compose the Tx Data
                let mut concatenated = bytes!("822bbe4c").to_vec(); // the selector of `transferMultipleAndCall(address recipientAndCallee, uint256[] calldata tokenIDs, uint256[] calldata amounts, bytes calldata data)`
                let recipient_and_callee_address_evm_word =
                    CONTRACT_TO_TRANSFER_AND_CALL_TO_ADDRESS.into_word();
                concatenated.append(recipient_and_callee_address_evm_word.to_vec().as_mut());

                let token_ids_offset = U256::from(128);
                concatenated.append(token_ids_offset.to_be_bytes_vec().as_mut());

                let transfer_amounts_offset = U256::from(224);
                concatenated.append(transfer_amounts_offset.to_be_bytes_vec().as_mut());

                let calldata_offset = U256::from(320);
                concatenated.append(calldata_offset.to_be_bytes_vec().as_mut());

                let token_ids_len = U256::from(2);
                concatenated.append(token_ids_len.to_be_bytes_vec().as_mut());
                concatenated.append(token1_id.to_be_bytes_vec().as_mut());
                concatenated.append(token2_id.to_be_bytes_vec().as_mut());

                let transfer_amounts_len = U256::from(2);
                concatenated.append(transfer_amounts_len.to_be_bytes_vec().as_mut());
                concatenated.append(transfer_amount.to_be_bytes_vec().as_mut());
                concatenated.append(transfer_amount.to_be_bytes_vec().as_mut());

                let mut calldata_concatenated: Vec<u8> = Vec::new();
                let mut word = [0; 32];

                word[28..].copy_from_slice(bytes!("65066c97").to_vec().as_slice()); // the selector of "transferMultipleTokensForAFee(address recipient, uint256[] calldata tokenIDs, uint256[] calldata amounts, uint256 fee)"
                calldata_concatenated.append(word.to_vec().as_mut());

                // calldata structure:
                // 0 - recipient's address
                // 1/32 - token ids offset (== 128)
                // 2/64 - transfer amounts offset (== 224)
                // 3/96 - fee amount
                // 4/128 - token ids len
                // 5/160 - token ids::token1_id
                // 6/192 - token ids::token2_id
                // 7/224 - transfer amounts len
                // 8/256 - transfer amounts::transfer_amount (1)
                // 9/288 - transfer amounts::transfer_amount (2)

                // Compose the Tx Data

                let recipient_address_evm_word = end_recipient_eoa.into_word();
                calldata_concatenated.append(recipient_address_evm_word.to_vec().as_mut());
                let token_ids_offset = U256::from(128);
                calldata_concatenated.append(token_ids_offset.to_be_bytes_vec().as_mut());

                let transfer_amounts_offset = U256::from(224);
                calldata_concatenated.append(transfer_amounts_offset.to_be_bytes_vec().as_mut());

                calldata_concatenated.append(fee_amount.to_be_bytes_vec().as_mut());

                let token_ids_len = U256::from(2);
                calldata_concatenated.append(token_ids_len.to_be_bytes_vec().as_mut());
                calldata_concatenated.append(token1_id.to_be_bytes_vec().as_mut());
                calldata_concatenated.append(token2_id.to_be_bytes_vec().as_mut());

                let transfer_amounts_len = U256::from(2);
                calldata_concatenated.append(transfer_amounts_len.to_be_bytes_vec().as_mut());
                calldata_concatenated.append(transfer_amount.to_be_bytes_vec().as_mut());
                calldata_concatenated.append(transfer_amount.to_be_bytes_vec().as_mut());

                let calldata_byte_length = U256::from(calldata_concatenated.len());
                concatenated.append(calldata_byte_length.to_be_bytes_vec().as_mut());
                concatenated.append(calldata_concatenated.as_mut());

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

        let recipient_token1_balance = evm.context.balance(token1_id, end_recipient_eoa).unwrap().0;
        assert_eq!(recipient_token1_balance, transfer_amount - fee_amount);

        let recipient_token2_balance = evm.context.balance(token2_id, end_recipient_eoa).unwrap().0;
        assert_eq!(recipient_token2_balance, transfer_amount - fee_amount);

        let recipient_token1_balance = evm
            .context
            .balance(token1_id, CONTRACT_TO_TRANSFER_AND_CALL_TO_ADDRESS)
            .unwrap()
            .0;
        assert_eq!(recipient_token1_balance, fee_amount);

        let recipient_token2_balance = evm
            .context
            .balance(token2_id, CONTRACT_TO_TRANSFER_AND_CALL_TO_ADDRESS)
            .unwrap()
            .0;
        assert_eq!(recipient_token2_balance, fee_amount);
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
}
