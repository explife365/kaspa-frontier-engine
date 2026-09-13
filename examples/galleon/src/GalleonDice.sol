// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Guess 1-6 on-chain dice. Testnet fun only.
interface IERC20Dice {
    function transferFrom(address from, address to, uint256 amount) external returns (bool);

    function transfer(address to, uint256 amount) external returns (bool);

    function balanceOf(address account) external view returns (uint256);
}

contract GalleonDice {
    IERC20Dice public immutable token;
    address public immutable house;

    uint256 public constant SIDES = 6;
    uint256 public immutable winNumerator;
    uint256 public immutable winDenominator;

    uint256 public minBet;
    uint256 public maxBet;
    uint256 public rolls;

    mapping(address => uint256) public playerNonce;

    event DiceRolled(address indexed player, uint8 guess, uint8 result, uint256 wager, uint256 payout);

    constructor(
        address _token,
        address _house,
        uint256 _minBet,
        uint256 _maxBet,
        uint256 _winNumerator,
        uint256 _winDenominator
    ) {
        require(_token != address(0) && _house != address(0), "zero addr");
        token = IERC20Dice(_token);
        house = _house;
        minBet = _minBet;
        maxBet = _maxBet;
        winNumerator = _winNumerator;
        winDenominator = _winDenominator;
    }

    function _roll(address player) internal returns (uint8 face) {
        uint256 nonce = playerNonce[player]++;
        uint256 entropy = uint256(
            keccak256(abi.encodePacked(block.prevrandao, block.timestamp, player, nonce, rolls))
        );
        face = uint8((entropy % SIDES) + 1);
    }

    function rollDice(uint8 guess, uint256 wager) external returns (uint8 result, uint256 payout) {
        require(guess >= 1 && guess <= SIDES, "guess");
        require(wager >= minBet && wager <= maxBet, "bet bounds");
        require(token.transferFrom(msg.sender, address(this), wager), "wager xfer");
        rolls++;
        result = _roll(msg.sender);
        payout = 0;
        if (guess == result) {
            payout = (wager * winNumerator) / winDenominator;
            require(token.balanceOf(address(this)) >= payout, "bankroll");
            require(token.transfer(msg.sender, payout), "payout xfer");
        }
        emit DiceRolled(msg.sender, guess, result, wager, payout);
    }

    function houseWithdraw(uint256 amount) external {
        require(msg.sender == house, "house");
        require(token.transfer(house, amount), "withdraw");
    }
}
