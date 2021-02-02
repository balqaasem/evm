use core::mem;
use alloc::collections::{BTreeMap, BTreeSet};
use primitive_types::{H160, H256, U256};
use crate::ExitError;
use crate::backend::{Basic, Apply, Log, Backend};
use crate::executor::stack::StackSubstateMetadata;

#[derive(Clone)]
struct MemoryStackAccount {
	pub basic: Basic,
	pub code: Option<Vec<u8>>,
	pub reset: bool,
}

pub struct MemoryStackSubstate<'config> {
	metadata: StackSubstateMetadata<'config>,
	parent: Option<Box<MemoryStackSubstate<'config>>>,
	logs: Vec<Log>,
	accounts: BTreeMap<H160, MemoryStackAccount>,
	storages: BTreeMap<(H160, H256), H256>,
	deletes: BTreeSet<H160>,
}

impl<'config> MemoryStackSubstate<'config> {
	pub fn new(metadata: StackSubstateMetadata<'config>) -> Self {
		Self {
			metadata,
			parent: None,
			logs: Vec::new(),
			accounts: BTreeMap::new(),
			storages: BTreeMap::new(),
			deletes: BTreeSet::new(),
		}
	}

	pub fn metadata(&self) -> &StackSubstateMetadata<'config> {
		&self.metadata
	}

	pub fn metadata_mut(&mut self) -> &mut StackSubstateMetadata<'config> {
		&mut self.metadata
	}

	// #[must_use]
	// pub fn deconstruct(
	// 	mut self
	// ) -> impl IntoIterator<Item=Apply<impl IntoIterator<Item=(H256, H256)>>> {
	// 	let mut applies = Vec::<Apply<BTreeMap<H256, H256>>>::new();

	// 	for (address, account) in self.modified {
	// 		if self.deleted.contains(&address) {
	// 			continue
	// 		}

	// 		applies.push(Apply::Modify {
	// 			address,
	// 			basic: account.basic,
	// 			code: account.code,
	// 			storage: account.storage,
	// 			reset_storage: account.reset_storage,
	// 		});
	// 	}

	// 	for address in self.deleted {
	// 		applies.push(Apply::Delete { address });
	// 	}

	// 	applies
	// }

	pub fn commit(&mut self) -> Result<(), ExitError> {
		let mut exited = *self.parent.take().expect("Cannot commit on root substate");
		mem::swap(&mut exited, self);

		self.metadata.swallow_commit(exited.metadata)?;
		self.accounts.append(&mut exited.accounts);
		self.storages.append(&mut exited.storages);
		self.deletes.append(&mut exited.deletes);

		Ok(())
	}

	pub fn discard(&mut self) -> Result<(), ExitError> {
		let mut exited = *self.parent.take().expect("Cannot discard on root substate");
		mem::swap(&mut exited, self);

		self.metadata.swallow_discard(exited.metadata)?;

		Ok(())
	}

	fn known_account(&self, address: &H160) -> Option<&MemoryStackAccount> {
		if let Some(account) = self.accounts.get(address) {
			Some(account)
		} else if let Some(parent) = self.parent.as_ref() {
			parent.known_account(address)
		} else {
			None
		}
	}

	pub fn known_basic(&self, address: &H160) -> Option<Basic> {
		self.known_account(address).map(|acc| acc.basic.clone())
	}

	pub fn known_code(&self, address: &H160) -> Option<Vec<u8>> {
		self.known_account(address).and_then(|acc| acc.code.clone())
	}

	pub fn known_empty(&self, address: &H160) -> Option<bool> {
		if let Some(account) = self.known_account(address) {
			if let Some(code) = &account.code {
				return Some(
					account.basic.balance == U256::zero() &&
						account.basic.nonce == U256::zero() &&
						code.len() == 0
				)
			}
		}

		None
	}

	fn account_mut<B: Backend>(&mut self, address: &H160, backend: &B) -> &mut MemoryStackAccount {
		if !self.accounts.contains_key(address) {
			let account = self.known_account(address)
				.cloned()
				.unwrap_or_else(|| MemoryStackAccount {
					basic: backend.basic(*address),
					code: None,
					reset: false,
				});
			self.accounts.insert(*address, account);
		}

		self.accounts.get_mut(address).expect("New account was just inserted")
	}

	pub fn inc_nonce<B: Backend>(&mut self, address: &H160, backend: &B) {
		self.account_mut(address, backend).basic.nonce += U256::one();
	}
}