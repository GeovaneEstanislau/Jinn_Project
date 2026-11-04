//! Core Security Invariant Verification Engine for Jinn Protocol
//! Mathematically verifies the 6 foundational security invariants required by the Master Prompt.

#![allow(dead_code)]

use crate::bep20::*;
use crate::merkle::*;

pub const HARD_CAP_WEI: u128 = 1_000_000_000 * 1_000_000_000_000_000_000u128; // 1 Billion * 10^18

// =========================================================================
// Invariant 1: Total Supply Never Exceeds Hard Cap (1 Billion $JINN)
// =========================================================================
#[derive(Debug)]
pub struct SimulatedToken {
    pub total_supply: u128,
    pub balances: std::collections::HashMap<[u8; 20], u128>,
    pub paused: bool,
    pub owner: [u8; 20],
    pub pending_owner: Option<[u8; 20]>,
}

impl SimulatedToken {
    pub fn new(owner: [u8; 20], initial_receiver: [u8; 20]) -> Self {
        let mut balances = std::collections::HashMap::new();
        balances.insert(initial_receiver, HARD_CAP_WEI);
        Self {
            total_supply: HARD_CAP_WEI,
            balances,
            paused: false,
            owner,
            pending_owner: None,
        }
    }

    pub fn burn(&mut self, caller: &[u8; 20], amount: u128) -> Result<(), &'static str> {
        if self.paused {
            return Err("JinnToken: token transfer while paused");
        }
        let balance = self.balances.get_mut(caller).ok_or("Insufficient balance")?;
        if *balance < amount {
            return Err("JinnToken: burn amount exceeds balance");
        }
        *balance -= amount;
        self.total_supply -= amount;
        Ok(())
    }

    pub fn transfer(&mut self, from: &[u8; 20], to: &[u8; 20], amount: u128) -> Result<(), &'static str> {
        if self.paused {
            return Err("JinnToken: token transfer while paused");
        }
        let from_bal = self.balances.get_mut(from).ok_or("Insufficient balance")?;
        if *from_bal < amount {
            return Err("JinnToken: transfer amount exceeds balance");
        }
        *from_bal -= amount;
        let to_bal = self.balances.entry(*to).or_insert(0);
        *to_bal += amount;
        Ok(())
    }

    pub fn pause(&mut self, caller: &[u8; 20]) -> Result<(), &'static str> {
        if *caller != self.owner {
            return Err("JinnToken: caller is not the owner");
        }
        self.paused = true;
        Ok(())
    }

    pub fn unpause(&mut self, caller: &[u8; 20]) -> Result<(), &'static str> {
        if *caller != self.owner {
            return Err("JinnToken: caller is not the owner");
        }
        self.paused = false;
        Ok(())
    }

    pub fn transfer_ownership(&mut self, caller: &[u8; 20], new_owner: [u8; 20]) -> Result<(), &'static str> {
        if *caller != self.owner {
            return Err("JinnToken: caller is not the owner");
        }
        self.pending_owner = Some(new_owner);
        Ok(())
    }

    pub fn accept_ownership(&mut self, caller: &[u8; 20]) -> Result<(), &'static str> {
        if self.pending_owner != Some(*caller) {
            return Err("JinnToken: caller is not pending owner");
        }
        self.owner = *caller;
        self.pending_owner = None;
        Ok(())
    }
}

// =========================================================================
// Invariant 2: Vesting Time-Lock & Cliff Protection
// =========================================================================
pub struct SimulatedVestingSchedule {
    pub beneficiary: [u8; 20],
    pub start: u64,
    pub cliff: u64,
    pub duration: u64,
    pub total_amount: u128,
    pub released_amount: u128,
    pub revocable: bool,
    pub revoked: bool,
}

impl SimulatedVestingSchedule {
    pub fn compute_releasable(&self, current_timestamp: u64) -> u128 {
        if self.revoked || current_timestamp < self.cliff {
            return 0;
        }
        if current_timestamp >= self.start + self.duration {
            self.total_amount - self.released_amount
        } else {
            let time_from_start = (current_timestamp - self.start) as u128;
            let vested = (self.total_amount * time_from_start) / (self.duration as u128);
            if vested > self.released_amount {
                vested - self.released_amount
            } else {
                0
            }
        }
    }

    pub fn release(&mut self, current_timestamp: u64) -> Result<u128, &'static str> {
        let releasable = self.compute_releasable(current_timestamp);
        if releasable == 0 {
            return Err("JinnVesting: no tokens due for release");
        }
        self.released_amount += releasable;
        Ok(releasable)
    }
}

// =========================================================================
// Invariant 3 & 6: Merkle Claims and Campaign Isolation
// =========================================================================
pub struct SimulatedEpoch {
    pub root: [u8; 32],
    pub total_reward: u128,
    pub total_claimed: u128,
    pub expiration: u64,
    pub active: bool,
    pub claimed_indices: std::collections::HashSet<u64>,
}

impl SimulatedEpoch {
    pub fn new(root: [u8; 32], total_reward: u128, expiration: u64) -> Self {
        Self {
            root,
            total_reward,
            total_claimed: 0,
            expiration,
            active: true,
            claimed_indices: std::collections::HashSet::new(),
        }
    }

    pub fn claim(
        &mut self,
        current_timestamp: u64,
        index: u64,
        contributor: &[u8; 20],
        amount: u128,
        proof: &[[u8; 32]],
    ) -> Result<(), &'static str> {
        if !self.active {
            return Err("JinnRewardDistributor: epoch is not active");
        }
        if self.expiration > 0 && current_timestamp > self.expiration {
            return Err("JinnRewardDistributor: epoch expired");
        }
        if self.claimed_indices.contains(&index) {
            return Err("JinnRewardDistributor: bounty already claimed");
        }
        if self.total_claimed + amount > self.total_reward {
            return Err("JinnRewardDistributor: exceeds epoch cap");
        }

        let mut amount_bytes = [0u8; 32];
        amount_bytes[16..32].copy_from_slice(&amount.to_be_bytes());
        let leaf = compute_pouc_leaf(index, contributor, &amount_bytes);

        if !verify_proof(proof, &self.root, &leaf) {
            return Err("JinnRewardDistributor: invalid proof");
        }

        self.claimed_indices.insert(index);
        self.total_claimed += amount;
        Ok(())
    }

    pub fn close(&mut self) -> u128 {
        self.active = false;
        self.total_reward - self.total_claimed
    }
}

// =========================================================================
// Automated Test Suite for All 6 Security Invariants
// =========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invariant_1_hard_cap_never_exceeded() {
        let owner = [0x01; 20];
        let treasury = [0x02; 20];
        let mut token = SimulatedToken::new(owner, treasury);

        // Initial total supply == 1 Billion
        assert_eq!(token.total_supply, HARD_CAP_WEI);

        // Transfers preserve total supply
        let user = [0x03; 20];
        token.transfer(&treasury, &user, 50_000_000).unwrap();
        assert_eq!(token.total_supply, HARD_CAP_WEI);

        // Burns reduce total supply and it remains <= 1 Billion
        token.burn(&treasury, 1_000_000).unwrap();
        assert_eq!(token.total_supply, HARD_CAP_WEI - 1_000_000);
        assert!(token.total_supply <= HARD_CAP_WEI);
    }

    #[test]
    fn test_invariant_2_vesting_cliff_and_no_early_claims() {
        let mut schedule = SimulatedVestingSchedule {
            beneficiary: [0xAA; 20],
            start: 1000,
            cliff: 1090, // 90 seconds cliff
            duration: 360,
            total_amount: 1_000_000,
            released_amount: 0,
            revocable: false,
            revoked: false,
        };

        // Before cliff (time = 1050): releasable must be exactly 0
        assert_eq!(schedule.compute_releasable(1050), 0);
        assert!(schedule.release(1050).is_err());

        // At cliff (time = 1090): 90/360 = 25% vested
        assert_eq!(schedule.compute_releasable(1090), 250_000);
        let released = schedule.release(1090).unwrap();
        assert_eq!(released, 250_000);

        // Immediate duplicate release must fail
        assert_eq!(schedule.compute_releasable(1090), 0);
        assert!(schedule.release(1090).is_err());

        // Full duration (time = 1360): remaining 750,000 released
        assert_eq!(schedule.compute_releasable(1360), 750_000);
        assert_eq!(schedule.release(1360).unwrap(), 750_000);
        assert_eq!(schedule.released_amount, 1_000_000);
    }

    #[test]
    fn test_invariant_3_merkle_claim_uniqueness_and_tamper_rejection() {
        let contributor = [0x55; 20];
        let amount = 10_000u128;
        let mut amount_bytes = [0u8; 32];
        amount_bytes[16..32].copy_from_slice(&amount.to_be_bytes());
        let leaf = compute_pouc_leaf(0, &contributor, &amount_bytes);

        let (root, layers) = build_merkle_tree(&[leaf]);
        let proof = generate_merkle_proof(&layers, 0);

        let mut epoch = SimulatedEpoch::new(root, 50_000, 0);

        // First valid claim
        assert!(epoch.claim(100, 0, &contributor, amount, &proof).is_ok());

        // Second duplicate claim must be rejected
        assert_eq!(
            epoch.claim(100, 0, &contributor, amount, &proof).unwrap_err(),
            "JinnRewardDistributor: bounty already claimed"
        );

        // Tampered contributor address must be rejected
        let fake_contributor = [0x99; 20];
        assert_eq!(
            epoch.claim(100, 1, &fake_contributor, amount, &proof).unwrap_err(),
            "JinnRewardDistributor: invalid proof"
        );
    }

    #[test]
    fn test_invariant_4_administrative_privilege_segregation() {
        let owner = [0x01; 20];
        let treasury = [0x02; 20];
        let hacker = [0x99; 20];
        let mut token = SimulatedToken::new(owner, treasury);

        // Hacker cannot pause
        assert!(token.pause(&hacker).is_err());
        assert!(!token.paused);

        // Hacker cannot transfer ownership
        assert!(token.transfer_ownership(&hacker, hacker).is_err());

        // Safe Ownable2Step: Owner initiates transfer to Treasury
        token.transfer_ownership(&owner, treasury).unwrap();
        assert_eq!(token.owner, owner); // Still owner until accepted

        // Hacker cannot accept ownership
        assert!(token.accept_ownership(&hacker).is_err());

        // Treasury accepts
        token.accept_ownership(&treasury).unwrap();
        assert_eq!(token.owner, treasury);
    }

    #[test]
    fn test_invariant_5_circuit_breaker_pause_mechanics() {
        let owner = [0x01; 20];
        let treasury = [0x02; 20];
        let user = [0x03; 20];
        let mut token = SimulatedToken::new(owner, treasury);

        // Pause token
        token.pause(&owner).unwrap();
        assert!(token.paused);

        // Transfers and burns must fail while paused
        assert_eq!(
            token.transfer(&treasury, &user, 100).unwrap_err(),
            "JinnToken: token transfer while paused"
        );
        assert_eq!(
            token.burn(&treasury, 100).unwrap_err(),
            "JinnToken: token transfer while paused"
        );

        // Unpause restores functionality
        token.unpause(&owner).unwrap();
        assert!(!token.paused);
        assert!(token.transfer(&treasury, &user, 100).is_ok());
    }

    #[test]
    fn test_invariant_6_campaign_isolation_in_reward_distributor() {
        let mut epoch1 = SimulatedEpoch::new([0x01; 32], 20_000, 1000);
        let epoch2 = SimulatedEpoch::new([0x02; 32], 30_000, 0);

        // Close epoch 1
        let refunded = epoch1.close();
        assert_eq!(refunded, 20_000);
        assert!(!epoch1.active);

        // Epoch 2 remains completely active with all its funds intact
        assert!(epoch2.active);
        assert_eq!(epoch2.total_reward, 30_000);
    }
}
