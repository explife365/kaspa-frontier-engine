// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Galleon L2 wiKAS release for TN10 SilverScript HTLC demo (int hashlock tag).
/// @dev Pairs with examples/silverscript/htlc.py payment_hash. Not a production bridge.
interface IERC20Bridge {
    function transfer(address to, uint256 amount) external returns (bool);

    function transferFrom(address from, address to, uint256 amount) external returns (bool);

    function balanceOf(address account) external view returns (uint256);
}

contract HtlcBridgeRelease {
    IERC20Bridge public immutable wikas;
    uint256 public immutable paymentHashInt;
    uint256 public immutable payoutAmount;

    bool public claimed;
    address public claimant;

    event Deposited(address indexed from, uint256 amount);
    event Claimed(address indexed to, uint256 preimage, uint256 amount);

    constructor(address _wikas, uint256 _paymentHashInt, uint256 _payoutAmount) {
        require(_wikas != address(0), "zero wikas");
        require(_payoutAmount > 0, "payout");
        wikas = IERC20Bridge(_wikas);
        paymentHashInt = _paymentHashInt;
        payoutAmount = _payoutAmount;
    }

    function vaultBalance() external view returns (uint256) {
        return wikas.balanceOf(address(this));
    }

    function deposit(uint256 amount) external {
        require(wikas.transferFrom(msg.sender, address(this), amount), "deposit xfer");
        emit Deposited(msg.sender, amount);
    }

    /// @notice Release wiKAS when preimage matches the L1 HTLC int payment_hash demo tag.
    function claimDemo(uint256 preimage) external {
        require(!claimed, "claimed");
        require(preimage == paymentHashInt, "hashlock");
        require(wikas.balanceOf(address(this)) >= payoutAmount, "underfunded");
        claimed = true;
        claimant = msg.sender;
        require(wikas.transfer(msg.sender, payoutAmount), "payout xfer");
        emit Claimed(msg.sender, preimage, payoutAmount);
    }
}
