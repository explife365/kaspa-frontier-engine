// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Heads-or-tails gTEST game. Testnet fun only — blockhash randomness is not secure for mainnet.
interface IERC20Flip {
    function transferFrom(address from, address to, uint256 amount) external returns (bool);

    function transfer(address to, uint256 amount) external returns (bool);

    function balanceOf(address account) external view returns (uint256);
}

contract GalleonCoinFlip {
    IERC20Flip public immutable token;
    address public immutable house;
    uint256 public immutable winNumerator;
    uint256 public immutable winDenominator;

    uint256 public minBet;
    uint256 public maxBet;
    uint256 public totalWagered;
    uint256 public totalPaidOut;
    uint256 public gamesPlayed;

    mapping(address => uint256) public playerNonce;

    event FlipPlayed(
        address indexed player,
        bool guessHeads,
        bool resultHeads,
        uint256 wager,
        uint256 payout
    );

    constructor(
        address _token,
        address _house,
        uint256 _minBet,
        uint256 _maxBet,
        uint256 _winNumerator,
        uint256 _winDenominator
    ) {
        require(_token != address(0) && _house != address(0), "zero addr");
        require(_winNumerator > _winDenominator, "payout");
        token = IERC20Flip(_token);
        house = _house;
        minBet = _minBet;
        maxBet = _maxBet;
        winNumerator = _winNumerator;
        winDenominator = _winDenominator;
    }

    function _roll(address player) internal returns (bool heads) {
        uint256 nonce = playerNonce[player]++;
        uint256 entropy = uint256(
            keccak256(
                abi.encodePacked(
                    block.prevrandao,
                    blockhash(block.number > 0 ? block.number - 1 : block.number),
                    player,
                    nonce,
                    gamesPlayed
                )
            )
        );
        heads = (entropy & 1) == 1;
    }

    function flip(bool guessHeads, uint256 wager) external returns (bool won, uint256 payout) {
        require(wager >= minBet && wager <= maxBet, "bet bounds");
        require(token.transferFrom(msg.sender, address(this), wager), "wager xfer");
        totalWagered += wager;
        gamesPlayed++;

        bool resultHeads = _roll(msg.sender);
        won = guessHeads == resultHeads;
        payout = 0;
        if (won) {
            payout = (wager * winNumerator) / winDenominator;
            require(token.balanceOf(address(this)) >= payout, "bankroll");
            require(token.transfer(msg.sender, payout), "payout xfer");
            totalPaidOut += payout;
        }
        emit FlipPlayed(msg.sender, guessHeads, resultHeads, wager, payout);
    }

    function houseWithdraw(uint256 amount) external {
        require(msg.sender == house, "house");
        require(token.transfer(house, amount), "withdraw");
    }
}
