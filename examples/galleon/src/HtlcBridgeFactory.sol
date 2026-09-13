// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {HtlcBridgeVaultSha256} from "./HtlcBridgeVaultSha256.sol";

/// @notice Deploy per-order SHA256 HTLC vaults with recipient binding and protocol fee on claim.
contract HtlcBridgeFactory {
    address public immutable wikas;
    address public immutable treasury;
    uint256 public immutable claimFeeBps;

    event VaultCreated(
        address indexed vault,
        bytes32 indexed paymentHash,
        address indexed recipient,
        uint256 payoutAmount,
        uint256 deadline
    );

    constructor(address _wikas, address _treasury, uint256 _claimFeeBps) {
        require(_wikas != address(0) && _treasury != address(0), "zero addr");
        require(_claimFeeBps < 10_000, "fee");
        wikas = _wikas;
        treasury = _treasury;
        claimFeeBps = _claimFeeBps;
    }

    function createVault(
        bytes32 paymentHash,
        uint256 payoutAmount,
        address recipient,
        uint256 deadline
    ) external returns (address vault) {
        vault = address(
            new HtlcBridgeVaultSha256(
                wikas,
                treasury,
                paymentHash,
                payoutAmount,
                recipient,
                deadline,
                claimFeeBps
            )
        );
        emit VaultCreated(vault, paymentHash, recipient, payoutAmount, deadline);
    }
}
