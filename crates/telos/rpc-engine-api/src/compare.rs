use std::collections::HashSet;
use std::fmt::Display;
use reth_primitives::{Address, B256, U256};
use reth_primitives::revm_primitives::HashMap;
use revm::db::AccountStatus;
use revm::{Database, Evm, State, TransitionAccount};
use reth_storage_errors::provider::ProviderError;
use crate::structs::{TelosAccountStateTableRow, TelosAccountTableRow};
use tracing::{debug, info};

/// This function compares the state diffs between revm and Telos EVM contract
pub fn compare_state_diffs<Ext, DB>(
    evm: &mut Evm<'_, Ext, &mut State<DB>>,
    revm_state_diffs: HashMap<Address, TransitionAccount>,
    statediffs_account: Vec<TelosAccountTableRow>,
    statediffs_accountstate: Vec<TelosAccountStateTableRow>,
    _new_addresses_using_create: Vec<(u64, U256)>,
    new_addresses_using_openwallet: Vec<(u64, U256)>,
) -> bool
where
    DB: Database,
    DB::Error: Into<ProviderError> + Display,
{
    if !revm_state_diffs.is_empty()
        || !statediffs_account.is_empty()
        || !statediffs_accountstate.is_empty()
    {
        let block_number = evm.block().number;

        debug!("{block_number} REVM State diffs: {:#?}", revm_state_diffs);
        debug!("{block_number} TEVM State diffs account: {:#?}", statediffs_account);
        debug!("{block_number} TEVM State diffs accountstate: {:#?}", statediffs_accountstate);
    }

    let revm_db: &mut &mut State<DB> = evm.db_mut();

    let mut new_addresses_using_openwallet_hashset = HashSet::new();
    for row in &new_addresses_using_openwallet {
        new_addresses_using_openwallet_hashset.insert(Address::from_word(B256::from(row.1)));
    }

    let mut statediffs_account_hashmap = HashSet::new();
    for row in &statediffs_account {
        statediffs_account_hashmap.insert(row.address);
    }
    let mut statediffs_accountstate_hashmap = HashSet::new();
    for row in &statediffs_accountstate {
        statediffs_accountstate_hashmap.insert((row.address,row.key));
    }

    for row in &statediffs_account {
        // Skip if address is created using openwallet and is empty
        if new_addresses_using_openwallet_hashset.contains(&row.address) && row.balance == U256::ZERO && row.nonce == 0 && row.code.len() == 0 {
            continue;
        }
        // Skip if row is removed
        if row.removed {
            continue
        }
        if let Ok(revm_row) = revm_db.basic(row.address) {
            if let Some(unwrapped_revm_row) = revm_row {
                // Check balance inequality
                if unwrapped_revm_row.balance != row.balance {
                    panic!("Difference in balance, address: {:?} - revm: {:?} - tevm: {:?}",row.address,unwrapped_revm_row.balance,row.balance);
                }
                // Check nonce inequality
                if unwrapped_revm_row.nonce != row.nonce {
                    panic!("Difference in nonce, address: {:?} - revm: {:?} - tevm: {:?}",row.address,unwrapped_revm_row.nonce,row.nonce);
                }
                // Check code size inequality
                if unwrapped_revm_row.clone().code.is_none() && row.code.len() != 0 || unwrapped_revm_row.clone().code.is_some() && !unwrapped_revm_row.clone().code.unwrap().is_empty() && row.code.len() == 0 {
                    match revm_db.code_by_hash(unwrapped_revm_row.code_hash) {
                        Ok(code_by_hash) =>
                            if (code_by_hash.is_empty() && row.code.len() != 0) || (!code_by_hash.is_empty() && row.code.len() == 0) {
                                panic!("Difference in code existence, address: {:?} - revm: {:?} - tevm: {:?}",row.address,code_by_hash,row.code)
                            },
                        Err(_) => panic!("Difference in code existence, address: {:?} - revm: {:?} - tevm: {:?}",row.address,unwrapped_revm_row.code,row.code),
                    }
                }
                // // Check code content inequality
                // if unwrapped_revm_row.clone().unwrap().code.is_some() && !unwrapped_revm_row.clone().unwrap().code.unwrap().is_empty() && unwrapped_revm_row.clone().unwrap().code.unwrap().bytes() != row.code {
                //     panic!("Difference in code content, revm: {:?}, tevm: {:?}",unwrapped_revm_row.clone().unwrap().code.unwrap().bytes(),row.code);
                // }
            } else {
                // Skip if address is empty on both sides
                if !(row.balance == U256::ZERO && row.nonce == 0 && row.code.len() == 0) {
                    if let Some(unwrapped_revm_state_diff) = revm_state_diffs.get(&row.address) {
                        if !(unwrapped_revm_state_diff.status == AccountStatus::Destroyed && row.nonce == 0 && row.balance == U256::ZERO && row.code.len() == 0) {
                            panic!("A modified `account` table row was found on both revm state and revm state diffs, but seems to be destroyed on just one side, address: {:?}",row.address);
                        }
                    } else {
                        panic!("A modified `account` table row was found on revm state, but contains no information, address: {:?}",row.address);
                    }
                }
            }
        } else {
            // Skip if address is empty on both sides
            if !(row.balance == U256::ZERO && row.nonce == 0 && row.code.len() == 0) {
                panic!("A modified `account` table row was not found on revm state, address: {:?}",row.address);
            }
        }
    }
    for row in &statediffs_accountstate {
        if let Ok(revm_row) = revm_db.storage(row.address, row.key) {
            // The values should match, but if it is removed, then the revm value should be zero
            if !(revm_row == row.value) && !(revm_row != U256::ZERO || row.removed == true) {
                panic!("Difference in value on revm storage, address: {:?}, key: {:?}, revm-value: {:?}, tevm-row: {:?}",row.address,row.key,revm_row,row);
            }
        } else {
            panic!("Key was not found on revm storage, address: {:?}, key: {:?}",row.address,row.key);
        }
    }

    for (address, account) in &revm_state_diffs {
        if let (Some(info),Some(previous_info)) = (account.info.clone(),account.previous_info.clone()) {
            if !(info.balance == previous_info.balance && info.nonce == previous_info.nonce && info.code_hash == previous_info.code_hash) {
                if statediffs_account_hashmap.get(address).is_none() {
                    panic!("A modified address was not found on tevm state diffs, address: {:?}",address); 
                }
            }
        } else {
            if statediffs_account_hashmap.get(address).is_none() {
                panic!("A modified address was not found on tevm state diffs, address: {:?}",address); 
            }
        }
        for (key,_) in account.storage.clone() {
            if statediffs_accountstate_hashmap.get(&(*address,key)).is_none() {
                panic!("A modified storage slot was not found on tevm state diffs, address: {:?}",address); 
            }
        }
    }
    
    return true
}
