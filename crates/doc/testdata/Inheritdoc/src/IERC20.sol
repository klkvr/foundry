interface IERC20 {
    /// @param recipient The recipient address
    /// @param amount The amount of tokens
    function transfer(address recipient, uint256 amount) external returns (bool);
}