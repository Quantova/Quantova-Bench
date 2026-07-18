//! The transaction workload, a realistic mix.

use std::collections::BTreeMap;

use qtv_account::Account as KeyAccount;
use qtv_codec::Encoder;
use qtv_node::execution::{execute_transfer, transfer_call, TRANSFER_GAS};
use qtv_node::fee::FeeParams;
use qtv_node::ledger::{Account, Ledger};
use qtv_tx::{sign, Body, Call, Wrapper};
use qtv_vm::asm::assemble;
use qtv_vm::interp::Interpreter;

/// The share of the block that is token calls, in basis points. The rest is
pub const TOKEN_SHARE_BPS: u64 = 3000;

/// The gas limit a token transfer is given. It is larger than a native transfer
pub const TOKEN_GAS: u64 = 8_000;

/// The starting balance every account is funded with, large enough that a long
const FUNDING: u64 = 1_000_000_000_000;

/// The transfer amount every transaction carries. Small and fixed so a run never
const AMOUNT: u64 = 1;

/// Whether a transaction is a native transfer or a token standard call.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Native,
    Token,
}

/// A signed transaction ready for the verifying path: the wrapper the validator
pub struct SignedTx {
    pub wrapper: Wrapper,
    pub pubkey: Vec<u8>,
    pub kind: Kind,
    pub sender: String,
    pub recipient: String,
    pub amount: u64,
    pub fee: u64,
}

/// The whole workload: the funded account set, the signed block, and the state
pub struct Workload {
    pub ledger: Ledger,
    pub block: Vec<SignedTx>,
    pub token_calls: usize,
    pub native_calls: usize,
}

fn derive_accounts(n: usize) -> Vec<KeyAccount> {
    let master = [81u8; 32];
    (0..n as u64)
        .map(|i| qtv_account::derive(&master, i))
        .collect()
}

/// The token contract address the token calls target. Its exact value does not
fn token_address() -> String {
    qtv_account::derive(&[112u8; 32], 0).address()
}

/// Encode a token transfer call: the recipient and the amount, the arguments the
fn token_call(token_addr: &str, recipient: &str, amount: u64) -> Call {
    let mut encoder = Encoder::new();
    encoder.put_bytes(recipient.as_bytes());
    encoder.put_u64(amount);
    Call::new(token_addr.to_string(), encoder.into_bytes())
}

impl Workload {
    /// Build a workload of `accounts` funded accounts and a block of `block_size`
    pub fn build(accounts: usize, block_size: usize) -> Workload {
        let fee = FeeParams::devnet().transfer_fee();
        let keys = derive_accounts(accounts);
        let token_addr = token_address();

        let mut ledger = Ledger::new();
        for key in &keys {
            ledger.set_account(
                &key.address(),
                &Account::funded(FUNDING, key.scheme(), key.public_key().to_vec()),
            );
        }

        let mut block = Vec::with_capacity(block_size);
        let mut token_calls = 0;
        let mut native_calls = 0;
        for i in 0..block_size {
            let sender_idx = i % accounts;
            let recipient_idx = (i + accounts / 2 + 1) % accounts;
            let sender = &keys[sender_idx];
            let recipient = &keys[recipient_idx];
            let kind = if pick_token(i) {
                Kind::Token
            } else {
                Kind::Native
            };
            let (call, gas) = match kind {
                Kind::Native => (transfer_call(&recipient.address(), AMOUNT), TRANSFER_GAS),
                Kind::Token => (
                    token_call(&token_addr, &recipient.address(), AMOUNT),
                    TOKEN_GAS,
                ),
            };
            let body = Body::new(sender.address(), 0, gas, u128::from(fee), call);
            let wrapper = sign(sender, &body);
            match kind {
                Kind::Token => token_calls += 1,
                Kind::Native => native_calls += 1,
            }
            block.push(SignedTx {
                wrapper,
                pubkey: sender.public_key().to_vec(),
                kind,
                sender: sender.address(),
                recipient: recipient.address(),
                amount: AMOUNT,
                fee,
            });
        }

        Workload {
            ledger,
            block,
            token_calls,
            native_calls,
        }
    }
}

/// Assign the token share deterministically and exactly: a transaction is a token
fn pick_token(i: usize) -> bool {
    (i as u64 % 10) < (TOKEN_SHARE_BPS / 1000)
}

/// The native transfer program, the fixed transfer the node runs on the qtv-vm.
pub fn execute_native(sender_bal: u64, recipient_bal: u64, amount: u64, fee: u64) -> (u64, u64) {
    let out = execute_transfer(sender_bal, recipient_bal, amount, fee, TRANSFER_GAS)
        .expect("a funded native transfer halts");
    (out.sender_balance, out.recipient_balance)
}

/// The issuer token standard transfer program: freeze and allowlist checks on
const TOKEN_PROGRAM: &str = "\
LDC r0, 0
LDC r1, 1
LDI r8, 2
SLOAD r9, r8
JNZ r9, reject
LDI r8, 3
SLOAD r9, r8
JZ r9, reject
LDI r8, 4
SLOAD r9, r8
JNZ r9, reject
LDI r8, 5
SLOAD r9, r8
JZ r9, reject
ADD r2, r0, r1
LDI r3, 0
SLOAD r4, r3
SUB r4, r4, r2
SSTORE r3, r4
LDI r5, 1
SLOAD r6, r5
ADD r6, r6, r0
SSTORE r5, r6
LDI r7, 6
SLOAD r8, r7
LDI r9, 1
ADD r8, r8, r9
SSTORE r7, r8
HALT
reject:
HALT";

/// Run the token standard transfer through the qtv-vm. Both parties are
pub fn execute_token(sender_bal: u64, recipient_bal: u64, amount: u64, fee: u64) -> (u64, u64) {
    let code = assemble(TOKEN_PROGRAM).expect("the token program assembles");
    let consts = [amount, fee];
    let mut storage = BTreeMap::new();
    storage.insert(0u64, sender_bal);
    storage.insert(1u64, recipient_bal);
    storage.insert(2u64, 0u64); // sender not frozen
    storage.insert(3u64, 1u64); // sender allowlisted
    storage.insert(4u64, 0u64); // recipient not frozen
    storage.insert(5u64, 1u64); // recipient allowlisted
    storage.insert(6u64, 0u64); // event counter
    let out = Interpreter::new(&code, &consts, TOKEN_GAS)
        .with_storage(storage)
        .run()
        .expect("a compliant token transfer halts");
    (
        out.storage.get(&0).copied().unwrap_or(0),
        out.storage.get(&1).copied().unwrap_or(0),
    )
}
