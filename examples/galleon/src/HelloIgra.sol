// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Smoke contract for Igra Galleon (38836). Not a USD stable.
contract HelloIgra {
    string public message;

    event MessageChanged(string oldMessage, string newMessage);

    constructor(string memory _message) {
        message = _message;
    }

    function setMessage(string memory _message) public {
        string memory oldMessage = message;
        message = _message;
        emit MessageChanged(oldMessage, _message);
    }
}
