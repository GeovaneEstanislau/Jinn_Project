// SPDX-License-Identifier: MIT
pragma solidity 0.8.24;

/**
 * @dev Interface of the BEP-20 standard as defined in the BSC specifications.
 * Extends the ERC-20 standard with the `getOwner()` method required by BSC.
 */
interface IBEP20 {
    function totalSupply() external view returns (uint256);
    function decimals() external view returns (uint8);
    function symbol() external view returns (string memory);
    function name() external view returns (string memory);
    function getOwner() external view returns (address);
    function balanceOf(address account) external view returns (uint256);
    function transfer(address recipient, uint256 amount) external returns (bool);
    function allowance(address _owner, address spender) external view returns (uint256);
    function approve(address spender, uint256 amount) external returns (bool);
    function transferFrom(address sender, address recipient, uint256 amount) external returns (bool);

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);
}

/**
 * @dev Interface for the EIP-2612 standard for gasless approvals via secp256k1 signatures.
 */
interface IERC20Permit {
    function permit(
        address owner,
        address spender,
        uint256 value,
        uint256 deadline,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external;

    function nonces(address owner) external view returns (uint256);
    function DOMAIN_SEPARATOR() external view returns (bytes32);
}

/**
 * @title JinnToken ($JINN)
 * @author Jinn Project Team
 * @notice The native ecosystem and utility token of the Jinn OS & Aladdin Desktop Multiverse.
 * @dev Implementation of the BEP-20 token standard on BNB Smart Chain (BSC) with:
 *      - Fixed Hard Cap of 1,000,000,000 $JINN (Zero mint post-deployment)
 *      - EIP-2612 Gasless Permit approvals
 *      - Deflationary Burn capability
 *      - Multi-Sig Emergency Pausable Circuit Breaker
 *      - Two-step Ownership transfer (Ownable2Step)
 */
contract JinnToken is IBEP20, IERC20Permit {
    // --- Token Identity & Constants ---
    string private constant _name = "Jinn Network Token";
    string private constant _symbol = "JINN";
    uint8 private constant _decimals = 18;
    uint256 public constant MAX_SUPPLY = 1_000_000_000 * 10**18; // 1 Billion $JINN

    // --- State Variables ---
    uint256 private _totalSupply;
    address private _owner;
    address private _pendingOwner;
    bool private _paused;

    mapping(address => uint256) private _balances;
    mapping(address => mapping(address => uint256)) private _allowances;

    // --- EIP-2612 Permit State ---
    mapping(address => uint256) private _nonces;
    bytes32 private immutable _cachedDomainSeparator;
    uint256 private immutable _cachedChainId;
    address private immutable _cachedThis;
    bytes32 private immutable _hashedName;
    bytes32 private immutable _hashedVersion;

    bytes32 private constant TYPE_HASH =
        keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");
    bytes32 private constant PERMIT_TYPEHASH =
        keccak256("Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)");

    // --- Events ---
    event OwnershipTransferStarted(address indexed previousOwner, address indexed newOwner);
    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event Paused(address account);
    event Unpaused(address account);

    // --- Modifiers ---
    modifier onlyOwner() {
        _checkOwner();
        _;
    }

    modifier whenNotPaused() {
        require(!_paused, "JinnToken: token transfer while paused");
        _;
    }

    /**
     * @notice Constructor initializes total supply and assigns to initial multi-sig/treasury receiver.
     * @param initialReceiver Address receiving the initial 1 Billion $JINN supply for vesting & pool distribution.
     */
    constructor(address initialReceiver) {
        require(initialReceiver != address(0), "JinnToken: initial receiver is zero address");

        _owner = msg.sender;
        emit OwnershipTransferred(address(0), msg.sender);

        _totalSupply = MAX_SUPPLY;
        _balances[initialReceiver] = MAX_SUPPLY;
        emit Transfer(address(0), initialReceiver, MAX_SUPPLY);

        // EIP-712 Caching
        _hashedName = keccak256(bytes(_name));
        _hashedVersion = keccak256(bytes("1"));
        _cachedChainId = block.chainid;
        _cachedThis = address(this);
        _cachedDomainSeparator = _buildDomainSeparator();
    }

    // --- BEP-20 View Functions ---

    function name() external pure override returns (string memory) {
        return _name;
    }

    function symbol() external pure override returns (string memory) {
        return _symbol;
    }

    function decimals() external pure override returns (uint8) {
        return _decimals;
    }

    function totalSupply() external view override returns (uint256) {
        return _totalSupply;
    }

    function getOwner() external view override returns (address) {
        return _owner;
    }

    function owner() external view returns (address) {
        return _owner;
    }

    function pendingOwner() external view returns (address) {
        return _pendingOwner;
    }

    function isPaused() external view returns (bool) {
        return _paused;
    }

    function balanceOf(address account) external view override returns (uint256) {
        return _balances[account];
    }

    function allowance(address holder, address spender) external view override returns (uint256) {
        return _allowances[holder][spender];
    }

    // --- Transfer & Approval Logic ---

    function transfer(address recipient, uint256 amount) external override whenNotPaused returns (bool) {
        _transfer(msg.sender, recipient, amount);
        return true;
    }

    function approve(address spender, uint256 amount) external override returns (bool) {
        _approve(msg.sender, spender, amount);
        return true;
    }

    function transferFrom(address sender, address recipient, uint256 amount) external override whenNotPaused returns (bool) {
        uint256 currentAllowance = _allowances[sender][msg.sender];
        if (currentAllowance != type(uint256).max) {
            require(currentAllowance >= amount, "JinnToken: insufficient allowance");
            unchecked {
                _approve(sender, msg.sender, currentAllowance - amount);
            }
        }
        _transfer(sender, recipient, amount);
        return true;
    }

    function increaseAllowance(address spender, uint256 addedValue) external returns (bool) {
        _approve(msg.sender, spender, _allowances[msg.sender][spender] + addedValue);
        return true;
    }

    function decreaseAllowance(address spender, uint256 subtractedValue) external returns (bool) {
        uint256 currentAllowance = _allowances[msg.sender][spender];
        require(currentAllowance >= subtractedValue, "JinnToken: decreased allowance below zero");
        unchecked {
            _approve(msg.sender, spender, currentAllowance - subtractedValue);
        }
        return true;
    }

    // --- Deflationary Burn Functions ---

    /**
     * @notice Burns `amount` tokens from caller's balance permanently.
     */
    function burn(uint256 amount) external whenNotPaused {
        _burn(msg.sender, amount);
    }

    /**
     * @notice Burns `amount` tokens from `account` using approved allowance.
     */
    function burnFrom(address account, uint256 amount) external whenNotPaused {
        uint256 currentAllowance = _allowances[account][msg.sender];
        if (currentAllowance != type(uint256).max) {
            require(currentAllowance >= amount, "JinnToken: burn amount exceeds allowance");
            unchecked {
                _approve(account, msg.sender, currentAllowance - amount);
            }
        }
        _burn(account, amount);
    }

    // --- EIP-2612 Permit Implementation ---

    function nonces(address account) external view override returns (uint256) {
        return _nonces[account];
    }

    function DOMAIN_SEPARATOR() public view override returns (bytes32) {
        return (block.chainid == _cachedChainId && address(this) == _cachedThis)
            ? _cachedDomainSeparator
            : _buildDomainSeparator();
    }

    function permit(
        address holder,
        address spender,
        uint256 value,
        uint256 deadline,
        uint8 v,
        bytes32 r,
        bytes32 s
    ) external override {
        require(block.timestamp <= deadline, "JinnToken: expired permit deadline");

        bytes32 structHash = keccak256(
            abi.encode(PERMIT_TYPEHASH, holder, spender, value, _nonces[holder]++, deadline)
        );
        bytes32 hash = keccak256(abi.encodePacked("\x19\x01", DOMAIN_SEPARATOR(), structHash));

        address signer = ecrecover(hash, v, r, s);
        require(signer != address(0) && signer == holder, "JinnToken: invalid permit signature");

        _approve(holder, spender, value);
    }

    // --- Emergency Pausable Controls ---

    function pause() external onlyOwner {
        _paused = true;
        emit Paused(msg.sender);
    }

    function unpause() external onlyOwner {
        _paused = false;
        emit Unpaused(msg.sender);
    }

    // --- Ownable2Step (Safe Governance Handover) ---

    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "JinnToken: new owner is zero address");
        _pendingOwner = newOwner;
        emit OwnershipTransferStarted(_owner, newOwner);
    }

    function acceptOwnership() external {
        require(msg.sender == _pendingOwner, "JinnToken: caller is not pending owner");
        emit OwnershipTransferred(_owner, msg.sender);
        _owner = msg.sender;
        _pendingOwner = address(0);
    }

    // --- Internal Helpers ---

    function _checkOwner() internal view {
        require(msg.sender == _owner, "JinnToken: caller is not the owner");
    }

    function _transfer(address from, address to, uint256 amount) internal {
        require(from != address(0), "JinnToken: transfer from zero address");
        require(to != address(0), "JinnToken: transfer to zero address");

        uint256 fromBalance = _balances[from];
        require(fromBalance >= amount, "JinnToken: transfer amount exceeds balance");
        unchecked {
            _balances[from] = fromBalance - amount;
            _balances[to] += amount;
        }

        emit Transfer(from, to, amount);
    }

    function _approve(address holder, address spender, uint256 amount) internal {
        require(holder != address(0), "JinnToken: approve from zero address");
        require(spender != address(0), "JinnToken: approve to zero address");

        _allowances[holder][spender] = amount;
        emit Approval(holder, spender, amount);
    }

    function _burn(address account, uint256 amount) internal {
        require(account != address(0), "JinnToken: burn from zero address");

        uint256 accountBalance = _balances[account];
        require(accountBalance >= amount, "JinnToken: burn amount exceeds balance");
        unchecked {
            _balances[account] = accountBalance - amount;
            _totalSupply -= amount;
        }

        emit Transfer(account, address(0), amount);
    }

    function _buildDomainSeparator() private view returns (bytes32) {
        return keccak256(
            abi.encode(TYPE_HASH, _hashedName, _hashedVersion, block.chainid, address(this))
        );
    }
}
