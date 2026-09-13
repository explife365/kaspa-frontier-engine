// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Constant-product pool with swap fees, LP shares, and protocol treasury.
/// @dev Galleon rehearsal only. Not production AMM.
interface IERC20Fee {
    function transferFrom(address from, address to, uint256 amount) external returns (bool);

    function transfer(address to, uint256 amount) external returns (bool);

    function balanceOf(address account) external view returns (uint256);
}

contract GalleonFeePool {
    IERC20Fee public immutable token0;
    IERC20Fee public immutable token1;
    address public immutable treasury;

    uint256 public immutable feeBps;
    uint256 public immutable protocolShareBps;

    uint256 public reserve0;
    uint256 public reserve1;
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;

    uint256 public treasury0;
    uint256 public treasury1;

    event LiquidityAdded(address indexed provider, uint256 amount0, uint256 amount1, uint256 sharesMinted);
    event LiquidityRemoved(address indexed provider, uint256 amount0, uint256 amount1, uint256 sharesBurned);
    event Swap(
        address indexed trader,
        bool zeroForOne,
        uint256 amountIn,
        uint256 amountOut,
        uint256 feeIn,
        uint256 protocolFeeIn
    );
    event TreasuryWithdrawn(address indexed to, uint256 amount0, uint256 amount1);

    constructor(
        address _token0,
        address _token1,
        address _treasury,
        uint256 _feeBps,
        uint256 _protocolShareBps
    ) {
        require(_token0 != address(0) && _token1 != address(0) && _treasury != address(0), "zero addr");
        require(_token0 != _token1, "same token");
        require(_feeBps > 0 && _feeBps < 10_000, "fee");
        require(_protocolShareBps <= 10_000, "protocol share");
        token0 = IERC20Fee(_token0);
        token1 = IERC20Fee(_token1);
        treasury = _treasury;
        feeBps = _feeBps;
        protocolShareBps = _protocolShareBps;
    }

    function getReserves() external view returns (uint256 r0, uint256 r1) {
        return (reserve0, reserve1);
    }

    function quoteSwap(uint256 amountIn, bool zeroForOne) public view returns (uint256 amountOut) {
        require(amountIn > 0, "amountIn");
        uint256 rIn = zeroForOne ? reserve0 : reserve1;
        uint256 rOut = zeroForOne ? reserve1 : reserve0;
        require(rIn > 0 && rOut > 0, "empty pool");
        uint256 amountInAfterFee = (amountIn * (10_000 - feeBps)) / 10_000;
        amountOut = (amountInAfterFee * rOut) / (rIn + amountInAfterFee);
        require(amountOut > 0 && amountOut < rOut, "insufficient liquidity");
    }

    function _mint(address to, uint256 shares) internal {
        totalSupply += shares;
        balanceOf[to] += shares;
    }

    function _burn(address from, uint256 shares) internal {
        balanceOf[from] -= shares;
        totalSupply -= shares;
    }

    function _sqrt(uint256 x) internal pure returns (uint256 z) {
        if (x == 0) {
            return 0;
        }
        uint256 y = x;
        z = (x + 1) / 2;
        while (z < y) {
            y = z;
            z = (x / z + z) / 2;
        }
        return y;
    }

    function addLiquidity(uint256 amount0, uint256 amount1) external returns (uint256 shares) {
        require(amount0 > 0 && amount1 > 0, "amounts");
        require(token0.transferFrom(msg.sender, address(this), amount0), "t0 xfer");
        require(token1.transferFrom(msg.sender, address(this), amount1), "t1 xfer");
        if (totalSupply == 0) {
            shares = _sqrt(amount0 * amount1);
            require(shares > 0, "shares");
        } else {
            uint256 s0 = (amount0 * totalSupply) / reserve0;
            uint256 s1 = (amount1 * totalSupply) / reserve1;
            shares = s0 < s1 ? s0 : s1;
            require(shares > 0, "shares");
        }
        _mint(msg.sender, shares);
        reserve0 += amount0;
        reserve1 += amount1;
        emit LiquidityAdded(msg.sender, amount0, amount1, shares);
    }

    function removeLiquidity(uint256 shares) external returns (uint256 amount0, uint256 amount1) {
        require(shares > 0 && shares <= balanceOf[msg.sender], "shares");
        amount0 = (shares * reserve0) / totalSupply;
        amount1 = (shares * reserve1) / totalSupply;
        require(amount0 > 0 && amount1 > 0, "amounts");
        _burn(msg.sender, shares);
        reserve0 -= amount0;
        reserve1 -= amount1;
        require(token0.transfer(msg.sender, amount0), "t0 out");
        require(token1.transfer(msg.sender, amount1), "t1 out");
        emit LiquidityRemoved(msg.sender, amount0, amount1, shares);
    }

    function swap(uint256 amountIn, bool zeroForOne, uint256 minOut) external {
        uint256 amountOut = quoteSwap(amountIn, zeroForOne);
        require(amountOut >= minOut, "minOut");
        uint256 feeIn = (amountIn * feeBps) / 10_000;
        uint256 protocolFeeIn = (feeIn * protocolShareBps) / 10_000;
        uint256 lpFeeIn = feeIn - protocolFeeIn;
        if (zeroForOne) {
            require(token0.transferFrom(msg.sender, address(this), amountIn), "t0 in");
            require(token1.transfer(msg.sender, amountOut), "t1 out");
            reserve0 += amountIn - protocolFeeIn;
            reserve1 -= amountOut;
            treasury0 += protocolFeeIn;
        } else {
            require(token1.transferFrom(msg.sender, address(this), amountIn), "t1 in");
            require(token0.transfer(msg.sender, amountOut), "t0 out");
            reserve1 += amountIn - protocolFeeIn;
            reserve0 -= amountOut;
            treasury1 += protocolFeeIn;
        }
        emit Swap(msg.sender, zeroForOne, amountIn, amountOut, feeIn, protocolFeeIn);
        // lpFeeIn remains in reserves (benefits LPs).
        assert(lpFeeIn + protocolFeeIn == feeIn);
    }

    function withdrawTreasury(address to) external {
        require(msg.sender == treasury, "treasury");
        uint256 a0 = treasury0;
        uint256 a1 = treasury1;
        treasury0 = 0;
        treasury1 = 0;
        if (a0 > 0) {
            require(token0.transfer(to, a0), "t0 treasury");
        }
        if (a1 > 0) {
            require(token1.transfer(to, a1), "t1 treasury");
        }
        emit TreasuryWithdrawn(to, a0, a1);
    }
}
