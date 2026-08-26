// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {CircleUsdc} from "./CircleUsdc.sol";

/// @notice Known Galleon (38836) tokens. Not a factory. Nothing here is Circle USDC.
library GalleonTokenBook {
    address internal constant GTEST = 0xBC5e27AB3CE2eDB243593CDa2437E5B30e0D5D7D;

    function igraTestUsdc() internal pure returns (address) {
        return CircleUsdc.GALLEON_TEST_USDC;
    }

    function isIgraTestUsdc(address token) internal pure returns (bool) {
        return token == CircleUsdc.GALLEON_TEST_USDC;
    }

    function isGtest(address token) internal pure returns (bool) {
        return token == GTEST;
    }

    function isCircleIssued(address token) internal pure returns (bool) {
        return CircleUsdc.isCircleIssued(CircleUsdc.GALLEON_CHAIN_ID, token);
    }
}
