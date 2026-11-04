// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import "./JinnToken.sol";

/**
 * @title JinnRewardDistributor
 * @author Jinn Project Team
 * @notice Automated Proof of Useful Contribution (PoUC) and Kernel Bounty Distributor.
 * @dev Distributes $JINN rewards to open-source contributors, driver authors, and security researchers
 *      using gas-efficient Merkle Tree proofs and multi-sig batch authorizations.
 *      Enforces campaign life-cycles, expiration timestamps, and transparent reclamation of unclaimable tokens.
 */
contract JinnRewardDistributor {
    IBEP20 public immutable token;
    address public owner;
    address public pendingOwner;

    /// @notice Distribution epoch configuration and status
    struct DistributionEpoch {
        bytes32 merkleRoot;
        uint256 totalRewardAmount;
        uint256 totalClaimedAmount;
        uint256 startTimestamp;
        uint256 expirationTimestamp; // 0 means no expiration
        bool active;
    }

    uint256 public currentEpoch;
    mapping(uint256 => DistributionEpoch) public epochs;
    // epochId => (claimIndex => claimed)
    mapping(uint256 => mapping(uint256 => bool)) public isClaimed;

    // --- Events ---
    event EpochCreated(
        uint256 indexed epochId,
        bytes32 merkleRoot,
        uint256 totalRewardAmount,
        uint256 startTimestamp,
        uint256 expirationTimestamp
    );
    event EpochClosed(uint256 indexed epochId, uint256 totalClaimed, uint256 unclaimedRefunded);
    event RewardClaimed(uint256 indexed epochId, uint256 indexed index, address indexed contributor, uint256 amount);
    event BatchRewardsSent(uint256 totalAmount, uint256 recipientCount);
    event UnclaimedReclaimed(uint256 indexed epochId, address indexed to, uint256 amount);
    event EmergencyWithdrawn(address indexed to, uint256 amount);
    event OwnershipTransferStarted(address indexed previousOwner, address indexed newOwner);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);

    modifier onlyOwner() {
        require(msg.sender == owner, "JinnRewardDistributor: caller is not the owner");
        _;
    }

    constructor(address tokenAddress) {
        require(tokenAddress != address(0), "JinnRewardDistributor: token address is zero");
        token = IBEP20(tokenAddress);
        owner = msg.sender;
        emit OwnershipTransferred(address(0), msg.sender);
    }

    /**
     * @notice Opens a new reward epoch for verified PoUC contributions.
     * @param merkleRoot Cryptographic root of the contributor tree (hash of: index, contributor, amount).
     * @param totalRewardAmount Total tokens allocated for this distribution epoch.
     * @param durationInSeconds Duration of the epoch in seconds (0 for no expiration).
     */
    function createEpoch(
        bytes32 merkleRoot,
        uint256 totalRewardAmount,
        uint256 durationInSeconds
    ) external onlyOwner {
        require(merkleRoot != bytes32(0), "JinnRewardDistributor: invalid root");
        require(totalRewardAmount > 0, "JinnRewardDistributor: invalid amount");
        require(token.balanceOf(address(this)) >= totalRewardAmount, "JinnRewardDistributor: contract balance too low");

        currentEpoch++;
        uint256 expiration = durationInSeconds > 0 ? block.timestamp + durationInSeconds : 0;

        epochs[currentEpoch] = DistributionEpoch({
            merkleRoot: merkleRoot,
            totalRewardAmount: totalRewardAmount,
            totalClaimedAmount: 0,
            startTimestamp: block.timestamp,
            expirationTimestamp: expiration,
            active: true
        });

        emit EpochCreated(currentEpoch, merkleRoot, totalRewardAmount, block.timestamp, expiration);
    }

    /**
     * @notice Allows a contributor to claim their $JINN reward using a valid Merkle proof.
     * @param epochId Target epoch ID.
     * @param index Contributor's unique index in this epoch's leaf list.
     * @param amount Token amount to be claimed.
     * @param proof Cryptographic proof sibling hashes.
     */
    function claimReward(
        uint256 epochId,
        uint256 index,
        uint256 amount,
        bytes32[] calldata proof
    ) external {
        DistributionEpoch storage epoch = epochs[epochId];
        require(epoch.active, "JinnRewardDistributor: epoch is not active");
        if (epoch.expirationTimestamp > 0) {
            require(block.timestamp <= epoch.expirationTimestamp, "JinnRewardDistributor: epoch expired");
        }
        require(!isClaimed[epochId][index], "JinnRewardDistributor: bounty already claimed");
        require(epoch.totalClaimedAmount + amount <= epoch.totalRewardAmount, "JinnRewardDistributor: exceeds epoch cap");

        // Verify Merkle Proof (leaf = keccak256(keccak256(index, contributor, amount)))
        bytes32 leaf = keccak256(bytes.concat(keccak256(abi.encode(index, msg.sender, amount))));
        require(_verifyProof(proof, epoch.merkleRoot, leaf), "JinnRewardDistributor: invalid proof");

        // Checks-Effects-Interactions
        isClaimed[epochId][index] = true;
        epoch.totalClaimedAmount += amount;

        require(token.transfer(msg.sender, amount), "JinnRewardDistributor: transfer failed");
        emit RewardClaimed(epochId, index, msg.sender, amount);
    }

    /**
     * @notice Formally closes an epoch and reclaims remaining unclaimed tokens back to the treasury.
     * @param epochId Epoch ID to close.
     */
    function closeEpoch(uint256 epochId) external onlyOwner {
        DistributionEpoch storage epoch = epochs[epochId];
        require(epoch.active, "JinnRewardDistributor: epoch not active");

        epoch.active = false;
        uint256 unclaimed = epoch.totalRewardAmount - epoch.totalClaimedAmount;

        if (unclaimed > 0) {
            require(token.transfer(owner, unclaimed), "JinnRewardDistributor: reclaim transfer failed");
            emit UnclaimedReclaimed(epochId, owner, unclaimed);
        }

        emit EpochClosed(epochId, epoch.totalClaimedAmount, unclaimed);
    }

    /**
     * @notice Multi-sig direct batch distribution for urgent grants or direct developer bounties.
     */
    function batchDistribute(address[] calldata recipients, uint256[] calldata amounts) external onlyOwner {
        require(recipients.length == amounts.length, "JinnRewardDistributor: array length mismatch");
        uint256 total = 0;

        for (uint256 i = 0; i < recipients.length; i++) {
            require(recipients[i] != address(0), "JinnRewardDistributor: zero address recipient");
            total += amounts[i];
            require(token.transfer(recipients[i], amounts[i]), "JinnRewardDistributor: transfer failed");
        }

        emit BatchRewardsSent(total, recipients.length);
    }

    /**
     * @notice Emergency withdrawal of funds in case of critical upgrade or migration.
     */
    function emergencyWithdraw(address to, uint256 amount) external onlyOwner {
        require(to != address(0), "JinnRewardDistributor: zero address");
        require(token.transfer(to, amount), "JinnRewardDistributor: transfer failed");
        emit EmergencyWithdrawn(to, amount);
    }

    // --- Merkle Verification Utility ---

    function _verifyProof(bytes32[] calldata proof, bytes32 root, bytes32 leaf) internal pure returns (bool) {
        bytes32 computedHash = leaf;
        for (uint256 i = 0; i < proof.length; i++) {
            bytes32 proofElement = proof[i];
            if (computedHash <= proofElement) {
                computedHash = keccak256(abi.encodePacked(computedHash, proofElement));
            } else {
                computedHash = keccak256(abi.encodePacked(proofElement, computedHash));
            }
        }
        return computedHash == root;
    }

    // --- View Helpers ---

    function getEpoch(uint256 epochId) external view returns (DistributionEpoch memory) {
        return epochs[epochId];
    }

    // --- Ownable2Step Governance ---

    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "JinnRewardDistributor: new owner is zero address");
        pendingOwner = newOwner;
        emit OwnershipTransferStarted(owner, newOwner);
    }

    function acceptOwnership() external {
        require(msg.sender == pendingOwner, "JinnRewardDistributor: caller is not pending owner");
        emit OwnershipTransferred(owner, msg.sender);
        owner = msg.sender;
        pendingOwner = address(0);
    }
}
