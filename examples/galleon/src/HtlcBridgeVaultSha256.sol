// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Per-order wiKAS vault: recipient claims with SHA256 preimage; protocol fee on payout.
interface IERC20BridgeVault {
    function transfer(address to, uint256 amount) external returns (bool);

    function transferFrom(address from, address to, uint256 amount) external returns (bool);

    function balanceOf(address account) external view returns (uint256);
}

contract HtlcBridgeVaultSha256 {
    IERC20BridgeVault public immutable wikas;
    address public immutable treasury;
    bytes32 public immutable paymentHash;
    uint256 public immutable payoutAmount;
    address public immutable recipient;
    uint256 public immutable deadline;
    uint256 public immutable claimFeeBps;

    bool public claimed;
    address public claimant;

    event Deposited(address indexed from, uint256 amount);
    event Claimed(address indexed recipient, address indexed caller, bytes preimage, uint256 net, uint256 fee);
    event Refunded(address indexed to, uint256 amount);

    constructor(
        address _wikas,
        address _treasury,
        bytes32 _paymentHash,
        uint256 _payoutAmount,
        address _recipient,
        uint256 _deadline,
        uint256 _claimFeeBps
    ) {
        require(_wikas != address(0) && _treasury != address(0) && _recipient != address(0), "zero addr");
        require(_paymentHash != bytes32(0), "zero hash");
        require(_payoutAmount > 0, "payout");
        require(_deadline > block.timestamp, "deadline");
        require(_claimFeeBps < 10_000, "fee");
        wikas = IERC20BridgeVault(_wikas);
        treasury = _treasury;
        paymentHash = _paymentHash;
        payoutAmount = _payoutAmount;
        recipient = _recipient;
        deadline = _deadline;
        claimFeeBps = _claimFeeBps;
    }

    function vaultBalance() external view returns (uint256) {
        return wikas.balanceOf(address(this));
    }

    function quoteClaim() external view returns (uint256 net, uint256 fee) {
        fee = (payoutAmount * claimFeeBps) / 10_000;
        net = payoutAmount - fee;
    }

    function deposit(uint256 amount) external {
        require(wikas.transferFrom(msg.sender, address(this), amount), "deposit xfer");
        emit Deposited(msg.sender, amount);
    }

    function claimSha256(bytes calldata preimage) external {
        require(!claimed, "claimed");
        require(block.timestamp <= deadline, "expired");
        require(sha256(preimage) == paymentHash, "hashlock");
        require(wikas.balanceOf(address(this)) >= payoutAmount, "underfunded");
        claimed = true;
        claimant = msg.sender;
        uint256 fee = (payoutAmount * claimFeeBps) / 10_000;
        uint256 net = payoutAmount - fee;
        require(wikas.transfer(recipient, net), "recipient xfer");
        if (fee > 0) {
            require(wikas.transfer(treasury, fee), "fee xfer");
        }
        emit Claimed(recipient, msg.sender, preimage, net, fee);
    }

    /// @notice After deadline, refund remaining wiKAS to depositor (operator path).
    function refund(address to) external {
        require(block.timestamp > deadline, "not expired");
        require(!claimed, "claimed");
        uint256 bal = wikas.balanceOf(address(this));
        require(bal > 0, "empty");
        claimed = true;
        require(wikas.transfer(to, bal), "refund xfer");
        emit Refunded(to, bal);
    }
}
