const { expect } = require("chai");
const { ethers } = require("hardhat");

describe("Jinn Network Token ($JINN) Comprehensive Test Suite", function () {
  let JinnToken, jinnToken;
  let JinnVesting, jinnVesting;
  let JinnRewardDistributor, distributor;
  let owner, treasury, alice, bob, charlie;

  const ONE_BILLION = ethers.parseEther("1000000000");

  beforeEach(async function () {
    [owner, treasury, alice, bob, charlie] = await ethers.getSigners();

    // 1. Deploy Token
    JinnToken = await ethers.getContractFactory("JinnToken");
    jinnToken = await JinnToken.deploy(treasury.address);
    await jinnToken.waitForDeployment();

    // 2. Deploy Vesting
    JinnVesting = await ethers.getContractFactory("JinnVesting");
    jinnVesting = await JinnVesting.deploy(await jinnToken.getAddress());
    await jinnVesting.waitForDeployment();

    // 3. Deploy Reward Distributor
    JinnRewardDistributor = await ethers.getContractFactory("JinnRewardDistributor");
    distributor = await JinnRewardDistributor.deploy(await jinnToken.getAddress());
    await distributor.waitForDeployment();
  });

  // ================================================================
  // INVARIANTE 1: O total de tokens nunca ultrapassa o Hard Cap (1B)
  // ================================================================
  describe("Invariant 1: Total Supply and Hard Cap Enforcement", function () {
    it("should strictly cap the total supply at 1 Billion and never exceed it", async function () {
      expect(await jinnToken.totalSupply()).to.equal(ONE_BILLION);
      expect(await jinnToken.balanceOf(treasury.address)).to.equal(ONE_BILLION);

      // Verify that no mint function exists in ABI
      expect(jinnToken.mint).to.be.undefined;

      // Burns should decrease total supply
      const burnAmount = ethers.parseEther("1000000");
      await jinnToken.connect(treasury).burn(burnAmount);
      expect(await jinnToken.totalSupply()).to.equal(ONE_BILLION - burnAmount);
      expect(await jinnToken.totalSupply()).to.be.lessThanOrEqual(ONE_BILLION);
    });

    it("should prevent burning more than account balance", async function () {
      await expect(
        jinnToken.connect(alice).burn(ethers.parseEther("1"))
      ).to.be.revertedWith("JinnToken: burn amount exceeds balance");
    });
  });

  // ================================================================
  // INVARIANTE 2: Tokens bloqueados não podem ser retirados antecipadamente
  // ================================================================
  describe("Invariant 2: Vesting Time-Lock and Cliff Invariants", function () {
    const VESTING_AMOUNT = ethers.parseEther("1000000"); // 1 Million
    const DURATION = 365 * 24 * 3600; // 1 year
    const CLIFF = 90 * 24 * 3600; // 90 days

    beforeEach(async function () {
      await jinnToken.connect(treasury).transfer(await jinnVesting.getAddress(), VESTING_AMOUNT);
    });

    it("should strictly return 0 releasable before the cliff date", async function () {
      const blockTimestamp = (await ethers.provider.getBlock("latest")).timestamp;
      const start = blockTimestamp + 10;
      const scheduleId = await jinnVesting.computeScheduleIdForAddressAndIndex(alice.address, 0);

      await jinnVesting.createVestingSchedule(alice.address, start, CLIFF, DURATION, VESTING_AMOUNT, false);

      // Time advance to 10 seconds before cliff
      await ethers.provider.send("evm_setNextBlockTimestamp", [start + CLIFF - 10]);
      await ethers.provider.send("evm_mine");

      expect(await jinnVesting.computeReleasableAmount(scheduleId)).to.equal(0);

      // Attempting to release before cliff must revert
      await expect(jinnVesting.release(scheduleId)).to.be.revertedWith("JinnVesting: no tokens due for release");
    });

    it("should prevent double withdrawal of the same vested tranche", async function () {
      const blockTimestamp = (await ethers.provider.getBlock("latest")).timestamp;
      const start = blockTimestamp + 10;
      const scheduleId = await jinnVesting.computeScheduleIdForAddressAndIndex(alice.address, 0);

      await jinnVesting.createVestingSchedule(alice.address, start, CLIFF, DURATION, VESTING_AMOUNT, false);

      // Move past duration (100% vested)
      await ethers.provider.send("evm_setNextBlockTimestamp", [start + DURATION + 100]);
      await ethers.provider.send("evm_mine");

      const releasable = await jinnVesting.computeReleasableAmount(scheduleId);
      expect(releasable).to.equal(VESTING_AMOUNT);

      // First release succeeds in claiming the full vested amount
      await jinnVesting.release(scheduleId);
      expect(await jinnToken.balanceOf(alice.address)).to.equal(VESTING_AMOUNT);

      // Immediate second release must strictly revert with no tokens due
      await expect(jinnVesting.release(scheduleId)).to.be.revertedWith("JinnVesting: no tokens due for release");
    });

    it("should never allow creating schedules exceeding available unallocated balance", async function () {
      const blockTimestamp = (await ethers.provider.getBlock("latest")).timestamp;
      await expect(
        jinnVesting.createVestingSchedule(bob.address, blockTimestamp, CLIFF, DURATION, ethers.parseEther("2000000"), false)
      ).to.be.revertedWith("JinnVesting: insufficient token balance");
    });
  });

  // ================================================================
  // INVARIANTE 3: Uma reivindicação Merkle não pode ser executada duas vezes
  // ================================================================
  describe("Invariant 3: Merkle Distribution Uniqueness and Replay Protection", function () {
    it("should reject duplicate claims and tamper attempts", async function () {
      const rewardAmount = ethers.parseEther("10000");
      await jinnToken.connect(treasury).transfer(await distributor.getAddress(), rewardAmount);

      // Single leaf tree for testing
      const index = 0;
      const amount = ethers.parseEther("5000");
      const leaf = ethers.keccak256(
        ethers.solidityPacked(
          ["bytes32"],
          [ethers.keccak256(ethers.AbiCoder.defaultAbiCoder().encode(["uint256", "address", "uint256"], [index, alice.address, amount]))]
        )
      );
      const merkleRoot = leaf; // Single leaf root

      // Create Epoch with 30 days duration
      await distributor.createEpoch(merkleRoot, rewardAmount, 30 * 24 * 3600);

      // First claim succeeds
      await distributor.connect(alice).claimReward(1, index, amount, []);
      expect(await jinnToken.balanceOf(alice.address)).to.equal(amount);

      // Second claim with same index must revert with already claimed
      await expect(
        distributor.connect(alice).claimReward(1, index, amount, [])
      ).to.be.revertedWith("JinnRewardDistributor: bounty already claimed");

      // Claim by bob on index 1 with invalid proof must revert with invalid proof
      await expect(
        distributor.connect(bob).claimReward(1, 1, amount, [])
      ).to.be.revertedWith("JinnRewardDistributor: invalid proof");
    });
  });

  // ================================================================
  // INVARIANTE 4: Usuários sem autorização não podem alterar parâmetros administrativos
  // ================================================================
  describe("Invariant 4: Administrative Privilege Segregation", function () {
    it("should prevent non-owners from calling administrative functions", async function () {
      await expect(jinnToken.connect(bob).pause()).to.be.revertedWith("JinnToken: caller is not the owner");
      await expect(jinnToken.connect(bob).unpause()).to.be.revertedWith("JinnToken: caller is not the owner");
      await expect(jinnToken.connect(bob).transferOwnership(bob.address)).to.be.revertedWith("JinnToken: caller is not the owner");

      await expect(
        jinnVesting.connect(bob).createVestingSchedule(bob.address, 100, 10, 100, 1000, false)
      ).to.be.revertedWith("JinnVesting: caller is not the owner");

      await expect(
        distributor.connect(bob).createEpoch(ethers.ZeroHash, 1000, 0)
      ).to.be.revertedWith("JinnRewardDistributor: caller is not the owner");
    });

    it("should enforce safe Ownable2Step transitions", async function () {
      await jinnToken.connect(owner).transferOwnership(alice.address);
      expect(await jinnToken.owner()).to.equal(owner.address);
      expect(await jinnToken.pendingOwner()).to.equal(alice.address);

      // Alice accepts
      await jinnToken.connect(alice).acceptOwnership();
      expect(await jinnToken.owner()).to.equal(alice.address);
      expect(await jinnToken.pendingOwner()).to.equal(ethers.ZeroAddress);
    });
  });

  // ================================================================
  // INVARIANTE 5: A pausa impede operações sensíveis
  // ================================================================
  describe("Invariant 5: Circuit Breaker Pause Mechanics", function () {
    it("should block transfers and burns when paused, and restore when unpaused", async function () {
      await jinnToken.connect(owner).pause();
      expect(await jinnToken.isPaused()).to.be.true;

      await expect(
        jinnToken.connect(treasury).transfer(alice.address, ethers.parseEther("100"))
      ).to.be.revertedWith("JinnToken: token transfer while paused");

      await expect(
        jinnToken.connect(treasury).burn(ethers.parseEther("100"))
      ).to.be.revertedWith("JinnToken: token transfer while paused");

      // Unpause restores operations
      await jinnToken.connect(owner).unpause();
      expect(await jinnToken.isPaused()).to.be.false;

      await expect(
        jinnToken.connect(treasury).transfer(alice.address, ethers.parseEther("100"))
      ).to.not.be.reverted;
    });
  });

  // ================================================================
  // INVARIANTE 6: Nenhuma falha em uma campanha compromete outras campanhas
  // ================================================================
  describe("Invariant 6: Campaign Isolation in Reward Distributor", function () {
    it("should isolate multiple epochs completely and allow reclaiming only expired/closed epoch delta", async function () {
      const EPOCH1_FUNDS = ethers.parseEther("20000");
      const EPOCH2_FUNDS = ethers.parseEther("30000");

      await jinnToken.connect(treasury).transfer(await distributor.getAddress(), EPOCH1_FUNDS + EPOCH2_FUNDS);

      const leaf1 = ethers.keccak256(ethers.toUtf8Bytes("leaf1"));
      const leaf2 = ethers.keccak256(ethers.toUtf8Bytes("leaf2"));

      await distributor.createEpoch(leaf1, EPOCH1_FUNDS, 100);
      await distributor.createEpoch(leaf2, EPOCH2_FUNDS, 0); // No expiration

      expect((await distributor.getEpoch(1)).totalRewardAmount).to.equal(EPOCH1_FUNDS);
      expect((await distributor.getEpoch(2)).totalRewardAmount).to.equal(EPOCH2_FUNDS);

      // Close epoch 1
      const initialOwnerBalance = await jinnToken.balanceOf(owner.address);
      await distributor.closeEpoch(1);

      // Epoch 1 is closed; owner received its unclaimed funds
      expect(await jinnToken.balanceOf(owner.address)).to.equal(initialOwnerBalance + EPOCH1_FUNDS);
      expect((await distributor.getEpoch(1)).active).to.be.false;

      // Epoch 2 remains completely active and unimpacted
      expect((await distributor.getEpoch(2)).active).to.be.true;
      expect((await distributor.getEpoch(2)).totalRewardAmount).to.equal(EPOCH2_FUNDS);
    });
  });

  // ================================================================
  // EIP-2612 Gasless Permit Signature Tests
  // ================================================================
  describe("EIP-2612 Permit Gasless Approvals", function () {
    it("should permit allowance via secp256k1 EIP-712 signature", async function () {
      const value = ethers.parseEther("500");
      const deadline = ethers.MaxUint256;
      const nonce = await jinnToken.nonces(owner.address);
      const chainId = (await ethers.provider.getNetwork()).chainId;
      const tokenAddress = await jinnToken.getAddress();

      const domain = {
        name: "Jinn Network Token",
        version: "1",
        chainId: chainId,
        verifyingContract: tokenAddress,
      };

      const types = {
        Permit: [
          { name: "owner", type: "address" },
          { name: "spender", type: "address" },
          { name: "value", type: "uint256" },
          { name: "nonce", type: "uint256" },
          { name: "deadline", type: "uint256" },
        ],
      };

      const message = {
        owner: owner.address,
        spender: alice.address,
        value: value,
        nonce: nonce,
        deadline: deadline,
      };

      const signature = await owner.signTypedData(domain, types, message);
      const sig = ethers.Signature.from(signature);

      await jinnToken.permit(owner.address, alice.address, value, deadline, sig.v, sig.r, sig.s);
      expect(await jinnToken.allowance(owner.address, alice.address)).to.equal(value);
      expect(await jinnToken.nonces(owner.address)).to.equal(nonce + 1n);
    });

    it("should reject expired permit signatures", async function () {
      const pastDeadline = 1000;
      await expect(
        jinnToken.permit(owner.address, alice.address, 100, pastDeadline, 27, ethers.ZeroHash, ethers.ZeroHash)
      ).to.be.revertedWith("JinnToken: expired permit deadline");
    });
  });

  // ================================================================
  // Vesting Revocation Mechanics
  // ================================================================
  describe("Vesting Revocation Mechanics", function () {
    it("should refund unvested tokens to owner on revocation", async function () {
      const start = (await ethers.provider.getBlock("latest")).timestamp + 10;
      const amount = ethers.parseEther("10000");
      await jinnToken.connect(treasury).transfer(await jinnVesting.getAddress(), amount);

      const scheduleId = await jinnVesting.computeScheduleIdForAddressAndIndex(bob.address, 0);
      await jinnVesting.createVestingSchedule(bob.address, start, 100, 1000, amount, true);

      // Non-owner cannot revoke
      await expect(jinnVesting.connect(bob).revoke(scheduleId)).to.be.revertedWith("JinnVesting: caller is not the owner");

      // Owner revokes unvested schedule
      const ownerBalBefore = await jinnToken.balanceOf(owner.address);
      await jinnVesting.connect(owner).revoke(scheduleId);
      const ownerBalAfter = await jinnToken.balanceOf(owner.address);
      expect(ownerBalAfter - ownerBalBefore).to.equal(amount);

      const sched = await jinnVesting.getVestingSchedule(scheduleId);
      expect(sched.revoked).to.be.true;
    });
  });

  // ================================================================
  // Batch Distribution & Emergency Operations
  // ================================================================
  describe("Batch Distribution and Emergency Operations", function () {
    it("should allow batch distribution to multiple recipients", async function () {
      const amount1 = ethers.parseEther("100");
      const amount2 = ethers.parseEther("200");
      await jinnToken.connect(treasury).transfer(await distributor.getAddress(), amount1 + amount2);

      await distributor.batchDistribute([alice.address, bob.address], [amount1, amount2]);
      expect(await jinnToken.balanceOf(alice.address)).to.equal(amount1);
      expect(await jinnToken.balanceOf(bob.address)).to.equal(amount2);
    });

    it("should allow emergency withdrawal by owner only", async function () {
      const amount = ethers.parseEther("50");
      await jinnToken.connect(treasury).transfer(await distributor.getAddress(), amount);

      await expect(
        distributor.connect(alice).emergencyWithdraw(alice.address, amount)
      ).to.be.revertedWith("JinnRewardDistributor: caller is not the owner");

      const ownerBalBefore = await jinnToken.balanceOf(owner.address);
      await distributor.connect(owner).emergencyWithdraw(owner.address, amount);
      const ownerBalAfter = await jinnToken.balanceOf(owner.address);
      expect(ownerBalAfter - ownerBalBefore).to.equal(amount);
    });
  });
});
