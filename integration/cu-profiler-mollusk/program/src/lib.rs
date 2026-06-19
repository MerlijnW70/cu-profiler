//! A tiny Solana program used to demonstrate and test cu-profiler's Mollusk
//! backend. It does a small, deterministic amount of compute so the harness
//! reports a non-trivial, stable compute-unit figure.

use solana_program::account_info::AccountInfo;
use solana_program::entrypoint;
use solana_program::entrypoint::ProgramResult;
use solana_program::msg;
use solana_program::pubkey::Pubkey;

entrypoint!(process_instruction);

/// Program entrypoint: log a marker, do a bit of work, log the result.
pub fn process_instruction(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
    _instruction_data: &[u8],
) -> ProgramResult {
    msg!("cu-profiler demo: begin");
    let mut acc: u64 = 0;
    for i in 0..5_000u64 {
        acc = acc.wrapping_add(i.wrapping_mul(3));
    }
    msg!("cu-profiler demo: result {}", acc);
    Ok(())
}
