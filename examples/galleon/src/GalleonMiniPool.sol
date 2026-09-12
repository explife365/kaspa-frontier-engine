// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Minimal constant-product pool for Galleon rehearsal (gTEST / wiKAS). Not production AMM.
interface IERC20Mini {
    function transferFrom(address from, address to, uint256 amount) external returns (bool);

    function transfer(address to, uint256 amount) external returns (bool);

    function balanceOf(address account) external view returns (uint256);
}

contract GalleonMiniPool {
    IERC20Mini public immutable token0;
    IERC20Mini public immutable token1;
    uint256 public reserve0;
    uint256 public reserve1;

    event LiquidityAdded(address indexed provider, uint256 amount0, uint256 amount1);
    event Swap(
        address indexed trader,
        bool zeroForOne,
        uint256 amountIn,
        uint256 amountOut,
        uint256 newReserve0,
        uint256 newReserve1
    );

    constructor(address _token0, address _token1) {
        require(_token0 != address(0) && _token1 != address(0), "zero token");
        require(_token0 != _token1, "same token");
        token0 = IERC20Mini(_token0);
        token1 = IERC20Mini(_token1);
    }

    function getReserves() external view returns (uint256 r0, uint256 r1) {
        return (reserve0, reserve1);
    }

    function quoteSwap(uint256 amountIn, bool zeroForOne) external view returns (uint256 amountOut) {
        require(amountIn > 0, "amountIn");
        uint256 rIn = zeroForOne ? reserve0 : reserve1;
        uint256 rOut = zeroForOne ? reserve1 : reserve0;
        require(rIn > 0 && rOut > 0, "empty pool");
        amountOut = (amountIn * rOut) / (rIn + amountIn);
        require(amountOut > 0 && amountOut < rOut, "insufficient liquidity");
    }

    function addLiquidity(uint256 amount0, uint256 amount1) external {
        require(amount0 > 0 && amount1 > 0, "amounts");
        require(token0.transferFrom(msg.sender, address(this), amount0), "t0 xfer");
        require(token1.transferFrom(msg.sender, address(this), amount1), "t1 xfer");
        reserve0 += amount0;
        reserve1 += amount1;
        emit LiquidityAdded(msg.sender, amount0, amount1);
    }

    function swap(uint256 amountIn, bool zeroForOne, uint256 minOut) external {
        uint256 amountOut = this.quoteSwap(amountIn, zeroForOne);
        require(amountOut >= minOut, "minOut");
        if (zeroForOne) {
            require(token0.transferFrom(msg.sender, address(this), amountIn), "t0 in");
            require(token1.transfer(msg.sender, amountOut), "t1 out");
            reserve0 += amountIn;
            reserve1 -= amountOut;
        } else {
            require(token1.transferFrom(msg.sender, address(this), amountIn), "t1 in");
            require(token0.transfer(msg.sender, amountOut), "t0 out");
            reserve1 += amountIn;
            reserve0 -= amountOut;
        }
        emit Swap(msg.sender, zeroForOne, amountIn, amountOut, reserve0, reserve1);
    }
}
