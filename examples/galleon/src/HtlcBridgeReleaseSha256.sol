// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Galleon L2 wiKAS release for TN10 SilverScript SHA256 HTLC demo.
/// @dev Pairs with examples/silverscript/htlc_sha256.py (bytes preimage, sha256 hashlock).
interface IERC20Bridge {
    function transfer(address to, uint256 amount) external returns (bool);

    function transferFrom(address from, address to, uint256 amount) external returns (bool);

    function balanceOf(address account) external view returns (uint256);
}

contract HtlcBridgeReleaseSha256 {
    IERC20Bridge public immutable wikas;
    bytes32 public immutable paymentHash;
    uint256 public immutable payoutAmount;

    bool public claimed;
    address public claimant;

    event Deposited(address indexed from, uint256 amount);
    event Claimed(address indexed to, bytes preimage, uint256 amount);

    constructor(address _wikas, bytes32 _paymentHash, uint256 _payoutAmount) {
        require(_wikas != address(0), "zero wikas");
        require(_paymentHash != bytes32(0), "zero hash");
        require(_payoutAmount > 0, "payout");
        wikas = IERC20Bridge(_wikas);
        paymentHash = _paymentHash;
        payoutAmount = _payoutAmount;
    }

    function vaultBalance() external view returns (uint256) {
        return wikas.balanceOf(address(this));
    }

    function deposit(uint256 amount) external {
        require(wikas.transferFrom(msg.sender, address(this), amount), "deposit xfer");
        emit Deposited(msg.sender, amount);
    }

    /// @notice Release wiKAS when sha256(preimage) matches the L1 HTLC payment hash.
    function claimSha256(bytes calldata preimage) external {
        require(!claimed, "claimed");
        require(sha256(preimage) == paymentHash, "hashlock");
        require(wikas.balanceOf(address(this)) >= payoutAmount, "underfunded");
        claimed = true;
        claimant = msg.sender;
        require(wikas.transfer(msg.sender, payoutAmount), "payout xfer");
        emit Claimed(msg.sender, preimage, payoutAmount);
    }
}
