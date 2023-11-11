/// @param user The user address
/// @param amount The amount of tokens
event SomeEvent(address user, uint256 amount);


/// @param user The user address
/// @param amount The amount of tokens
struct SomeStruct {
    address addr;
    uint256 amount;
    string description;
}

/// @param code Error code
/// @param message Error message
error SomeError(uint256 code, string message);