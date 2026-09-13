// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Timed gTEST jackpot — buy tickets, draw a winner. Testnet fun only.
interface IERC20Jackpot {
    function transferFrom(address from, address to, uint256 amount) external returns (bool);

    function transfer(address to, uint256 amount) external returns (bool);
}

contract GalleonJackpot {
    IERC20Jackpot public immutable token;
    address public immutable house;
    uint256 public immutable houseFeeBps;

    uint256 public ticketPrice;
    uint256 public roundId;
    uint256 public roundEndsAt;
    uint256 public pot;

    address[] internal entrants;

    event TicketBought(address indexed player, uint256 roundId, uint256 ticketIndex);
    event RoundDrawn(uint256 roundId, address indexed winner, uint256 pot, uint256 houseFee);

    constructor(
        address _token,
        address _house,
        uint256 _ticketPrice,
        uint256 _roundDurationSec,
        uint256 _houseFeeBps
    ) {
        require(_token != address(0) && _house != address(0), "zero addr");
        require(_houseFeeBps < 10_000, "fee");
        token = IERC20Jackpot(_token);
        house = _house;
        ticketPrice = _ticketPrice;
        houseFeeBps = _houseFeeBps;
        roundEndsAt = block.timestamp + _roundDurationSec;
    }

    function ticketCount() external view returns (uint256) {
        return entrants.length;
    }

    function buyTicket() external {
        require(block.timestamp < roundEndsAt, "round ended");
        require(token.transferFrom(msg.sender, address(this), ticketPrice), "ticket xfer");
        entrants.push(msg.sender);
        pot += ticketPrice;
        emit TicketBought(msg.sender, roundId, entrants.length - 1);
    }

    function draw(uint256 roundDurationSec) external {
        require(block.timestamp >= roundEndsAt, "round active");
        require(entrants.length > 0, "no tickets");
        uint256 entropy = uint256(
            keccak256(abi.encodePacked(block.prevrandao, blockhash(block.number - 1), roundId, pot))
        );
        address winner = entrants[entropy % entrants.length];
        uint256 fee = (pot * houseFeeBps) / 10_000;
        uint256 prize = pot - fee;
        if (fee > 0) {
            require(token.transfer(house, fee), "fee xfer");
        }
        require(token.transfer(winner, prize), "prize xfer");
        emit RoundDrawn(roundId, winner, prize, fee);
        delete entrants;
        pot = 0;
        roundId++;
        roundEndsAt = block.timestamp + roundDurationSec;
    }
}
