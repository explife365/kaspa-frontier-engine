// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Owner-push iKAS faucet (relayer pattern). Does not mint iKAS.
/// Users who need gas cannot call drip() themselves — Igra's official faucet
/// pays 21_000 gas and sends native iKAS. Same idea as a CEX/relayer USDT send.
contract GalleonIkasFaucet {
    address public immutable owner;
    uint256 public immutable dripAmount;
    uint256 public immutable cooldown;
    mapping(address => uint256) public lastDripAt;

    event Funded(address indexed from, uint256 amount);
    event Dripped(address indexed to, uint256 amount);

    constructor(uint256 dripAmount_, uint256 cooldown_) payable {
        owner = msg.sender;
        dripAmount = dripAmount_;
        cooldown = cooldown_;
    }

    receive() external payable {
        emit Funded(msg.sender, msg.value);
    }

    /// @dev Relayer/owner pays gas. `to` can have a zero balance.
    function dripTo(address to) external {
        require(msg.sender == owner, "owner");
        require(to != address(0), "zero");
        require(dripAmount > 0, "zero drip");
        require(address(this).balance >= dripAmount, "empty");
        uint256 last = lastDripAt[to];
        require(last == 0 || block.timestamp >= last + cooldown, "cooldown");
        lastDripAt[to] = block.timestamp;
        (bool ok, ) = payable(to).call{value: dripAmount}("");
        require(ok, "send");
        emit Dripped(to, dripAmount);
    }

    function withdraw(uint256 amount) external {
        require(msg.sender == owner, "owner");
        (bool ok, ) = payable(owner).call{value: amount}("");
        require(ok, "send");
    }
}
