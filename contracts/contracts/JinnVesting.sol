// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

import "./JinnToken.sol";

/**
 * @title JinnVesting
 * @author Jinn Project Team
 * @notice Multi-schedule linear vesting vault with cliff for Jinn Network ($JINN).
 * @dev Manages long-term token releases for Core Maintainers, Ecosystem Treasury, and Grants.
 *      Enforces mathematical precision, prevents early claims, and guarantees no double releases.
 */
contract JinnVesting {
    struct VestingSchedule {
        bool initialized;
        address beneficiary;
        uint256 cliff;        // Timestamp when cliff ends and vesting starts
        uint256 start;        // Start timestamp of vesting
        uint256 duration;     // Duration in seconds of linear vesting after start
        uint256 totalAmount;  // Total tokens allocated
        uint256 releasedAmount; // Tokens already claimed
        bool revocable;       // Whether owner can revoke unvested portion
        bool revoked;         // Revocation flag
    }

    IBEP20 public immutable token;
    address public owner;
    address public pendingOwner;

    bytes32[] private _scheduleIds;
    mapping(bytes32 => VestingSchedule) private _schedules;
    mapping(address => uint256) private _holdersVestingCount;
    uint256 private _totalAllocated;

    // --- Events ---
    event VestingScheduleCreated(
        bytes32 indexed scheduleId,
        address indexed beneficiary,
        uint256 totalAmount,
        uint256 start,
        uint256 cliff,
        uint256 duration,
        bool revocable
    );
    event TokensReleased(bytes32 indexed scheduleId, address indexed beneficiary, uint256 amount);
    event VestingScheduleRevoked(bytes32 indexed scheduleId, uint256 refundAmount);
    event OwnershipTransferStarted(address indexed previousOwner, address indexed newOwner);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);

    modifier onlyOwner() {
        require(msg.sender == owner, "JinnVesting: caller is not the owner");
        _;
    }

    constructor(address tokenAddress) {
        require(tokenAddress != address(0), "JinnVesting: token address is zero");
        token = IBEP20(tokenAddress);
        owner = msg.sender;
        emit OwnershipTransferred(address(0), msg.sender);
    }

    /**
     * @notice Creates a new vesting schedule for a beneficiary.
     * @param beneficiary Address entitled to release the vested tokens.
     * @param start Timestamp when the vesting starts.
     * @param cliffDuration Duration in seconds until the first tokens unlock.
     * @param duration Total vesting duration in seconds.
     * @param amount Total $JINN tokens allocated.
     * @param revocable Whether owner can cancel unvested tokens.
     */
    function createVestingSchedule(
        address beneficiary,
        uint256 start,
        uint256 cliffDuration,
        uint256 duration,
        uint256 amount,
        bool revocable
    ) external onlyOwner returns (bytes32) {
        require(beneficiary != address(0), "JinnVesting: beneficiary is zero address");
        require(duration > 0, "JinnVesting: duration must be > 0");
        require(amount > 0, "JinnVesting: amount must be > 0");
        require(cliffDuration <= duration, "JinnVesting: cliff cannot exceed duration");

        uint256 availableBalance = token.balanceOf(address(this)) - _totalAllocated;
        require(availableBalance >= amount, "JinnVesting: insufficient token balance");

        bytes32 scheduleId = computeScheduleIdForAddressAndIndex(beneficiary, _holdersVestingCount[beneficiary]);
        uint256 cliff = start + cliffDuration;

        _schedules[scheduleId] = VestingSchedule({
            initialized: true,
            beneficiary: beneficiary,
            cliff: cliff,
            start: start,
            duration: duration,
            totalAmount: amount,
            releasedAmount: 0,
            revocable: revocable,
            revoked: false
        });

        _totalAllocated += amount;
        _holdersVestingCount[beneficiary]++;
        _scheduleIds.push(scheduleId);

        emit VestingScheduleCreated(scheduleId, beneficiary, amount, start, cliff, duration, revocable);
        return scheduleId;
    }

    /**
     * @notice Releases all currently vested and claimable tokens for a schedule.
     */
    function release(bytes32 scheduleId) external {
        VestingSchedule storage schedule = _schedules[scheduleId];
        require(schedule.initialized, "JinnVesting: schedule does not exist");
        require(!schedule.revoked, "JinnVesting: schedule is revoked");

        uint256 releasable = computeReleasableAmount(scheduleId);
        require(releasable > 0, "JinnVesting: no tokens due for release");

        // Checks-Effects-Interactions
        schedule.releasedAmount += releasable;
        _totalAllocated -= releasable;

        require(token.transfer(schedule.beneficiary, releasable), "JinnVesting: transfer failed");
        emit TokensReleased(scheduleId, schedule.beneficiary, releasable);
    }

    /**
     * @notice Revokes a revocable schedule, returning unvested tokens to owner.
     */
    function revoke(bytes32 scheduleId) external onlyOwner {
        VestingSchedule storage schedule = _schedules[scheduleId];
        require(schedule.initialized, "JinnVesting: schedule does not exist");
        require(schedule.revocable, "JinnVesting: schedule is not revocable");
        require(!schedule.revoked, "JinnVesting: already revoked");

        uint256 releasable = computeReleasableAmount(scheduleId);
        if (releasable > 0) {
            schedule.releasedAmount += releasable;
            _totalAllocated -= releasable;
            require(token.transfer(schedule.beneficiary, releasable), "JinnVesting: transfer failed");
            emit TokensReleased(scheduleId, schedule.beneficiary, releasable);
        }

        uint256 unvested = schedule.totalAmount - schedule.releasedAmount;
        schedule.revoked = true;
        _totalAllocated -= unvested;

        require(token.transfer(owner, unvested), "JinnVesting: refund transfer failed");
        emit VestingScheduleRevoked(scheduleId, unvested);
    }

    /**
     * @notice Computes vested tokens claimable at current timestamp.
     */
    function computeReleasableAmount(bytes32 scheduleId) public view returns (uint256) {
        VestingSchedule storage schedule = _schedules[scheduleId];
        if (!schedule.initialized || schedule.revoked) return 0;
        if (block.timestamp < schedule.cliff) return 0;

        if (block.timestamp >= schedule.start + schedule.duration) {
            return schedule.totalAmount - schedule.releasedAmount;
        } else {
            uint256 timeFromStart = block.timestamp - schedule.start;
            uint256 vested = (schedule.totalAmount * timeFromStart) / schedule.duration;
            return vested - schedule.releasedAmount;
        }
    }

    function getVestingSchedule(bytes32 scheduleId) external view returns (VestingSchedule memory) {
        return _schedules[scheduleId];
    }

    function computeScheduleIdForAddressAndIndex(address holder, uint256 index) public pure returns (bytes32) {
        return keccak256(abi.encodePacked(holder, index));
    }

    function getVestingScheduleByBeneficiaryAndIndex(address beneficiary, uint256 index) external view returns (VestingSchedule memory) {
        bytes32 id = computeScheduleIdForAddressAndIndex(beneficiary, index);
        return _schedules[id];
    }

    function getHoldersVestingCount(address holder) external view returns (uint256) {
        return _holdersVestingCount[holder];
    }

    function getVestingSchedulesCount() external view returns (uint256) {
        return _scheduleIds.length;
    }

    function getTotalAllocated() external view returns (uint256) {
        return _totalAllocated;
    }

    // --- Ownable2Step Governance ---

    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "JinnVesting: new owner is zero address");
        pendingOwner = newOwner;
        emit OwnershipTransferStarted(owner, newOwner);
    }

    function acceptOwnership() external {
        require(msg.sender == pendingOwner, "JinnVesting: caller is not pending owner");
        emit OwnershipTransferred(owner, msg.sender);
        owner = msg.sender;
        pendingOwner = address(0);
    }
}
