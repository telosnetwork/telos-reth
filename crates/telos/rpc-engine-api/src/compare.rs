use crate::structs::{TelosAccountStateTableRow, TelosAccountTableRow};
use reth_primitives::revm_primitives::HashMap;
use reth_primitives::{Address, B256, U256};
use reth_storage_errors::provider::ProviderError;
use revm::db::states::StorageSlot;
use revm::{Database, Evm, State, TransitionAccount};
use std::collections::HashSet;
use std::fmt::Display;

/// This function compares the state diffs between revm and Telos EVM contract
pub fn compare_state_diffs<Ext, DB>(
    evm: &mut Evm<'_, Ext, &mut State<DB>>,
    revm_diffs: HashMap<Address, TransitionAccount>,
    diffs_account: Vec<TelosAccountTableRow>,
    diffs_accountstate: Vec<TelosAccountStateTableRow>,
    addr_create: Vec<(u64, U256)>,
    addr_openwallet: Vec<(u64, U256)>,
) -> Result<(), &'static str>
where
    DB: Database,
    DB::Error: Into<ProviderError> + Display,
{
    println!("REVM State diffs: {:?}", revm_diffs);
    println!("TEVM State diffs account: {:?}", diffs_account);
    println!("TEVM State diffs accountstate: {:?}", diffs_accountstate);

    let addr_openwallet_set: HashSet<Address> =
        HashSet::from_iter(addr_openwallet.iter().map(|&(_, v)| Address::from_word(B256::from(v))));

    let is_not_empty_openwallet_account = |row: &&TelosAccountTableRow| {
        !(addr_openwallet_set.contains(&row.address)
            && row.balance == U256::ZERO
            && row.nonce == 0
            && row.code.len() == 0)
    };

    // There is a situation that revm produce a state diff for an account
    // but the critical values (balance,nonce,code,storage) are not actually changed
    // and we should exclude them to make comparison
    let is_changed_revm_account = |&(_, account): &(&Address, &TransitionAccount)| {
        let TransitionAccount { storage_was_destroyed, storage, info, previous_info, .. } = account;

        let (Some(info), Some(previous_info)) = (info, previous_info) else {
            return false;
        };

        storage.is_empty()
            && !storage_was_destroyed
            && info.balance == previous_info.balance
            && info.nonce == previous_info.nonce
            && info.code_hash == previous_info.code_hash
    };

    let without_empty_openwallet_accounts =
        diffs_account.iter().filter(is_not_empty_openwallet_account).collect::<Vec<_>>();

    let changed_revm_accounts =
        revm_diffs.iter().filter(is_changed_revm_account).map(|(&address, _)| address);

    let modified_accounts = HashSet::<Address>::from_iter(
        without_empty_openwallet_accounts
            .iter()
            .map(|row| row.address)
            .chain(changed_revm_accounts),
    );

    if modified_accounts.len() != revm_diffs.len() {
        return Err("Difference in number of modified addresses");
    }

    for row in without_empty_openwallet_accounts {
        let Some(revm_side_row) = revm_diffs.get(&row.address) else {
            return Err("A modified `account` table row not found on revm state diffs");
        };

        let Some(info) = revm_side_row.info.as_ref() else {
            return Err("A modified `account` table row found on revm state diffs, but contains no information");
        };

        if info.balance != row.balance {
            return Err("Difference in balance");
        }

        if info.nonce != row.nonce {
            return Err("Difference in nonce");
        }

        if info.code.is_none() != row.code.is_empty() {
            return Err("Difference in code existence");
        }

        // TODO: Check code content inequality?
    }

    for row in diffs_accountstate {
        let Some(revm_side_row) = revm_diffs.get(&row.address) else {
            return Err("A modified `accountstate` table row not found on revm state diffs");
        };

        match revm_side_row.storage.get(&row.key) {
            Some(&StorageSlot { present_value, .. }) => {
                if present_value == U256::ZERO && row.removed || present_value == row.value {
                    continue;
                }

                let block = evm.block().number;
                let TelosAccountStateTableRow { address, key, value, .. } = row;
                // TODO: Is it ok just to print values, insead of returning them as error?
                println!(
                    r#"
Storage:
    block: {block}
    address: {address}
    key: {key}
    revm value: {present_value}
    evm value: {value}
"#
                );
                return Err("Difference in value on modified storage");
            }
            None => {
                // The TEVM state diffs will include all storage "modifications" even if the value is the same
                // so if it's not in the REVM diffs, we need to check if the REVM db matches the TEVM state diff
                let revm_db: &mut &mut State<DB> = evm.db_mut();

                let revm_row = match revm_db.storage(row.address, row.key) {
                    Ok(revm_row) => revm_row,
                    Err(error) => {
                        println!("{error}");
                        return Err("Key not found on revm storage");
                    }
                };

                if revm_row == U256::ZERO && !row.removed || revm_row != row.value {
                    return Err("Difference in value on revm storage");
                }
            }
        }
    }

    for _row in addr_create {}

    for _row in addr_openwallet {}

    // TODO: Check balance and nonce

    Ok(())
}
