// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";

contract ZeroUSD is ERC20 {
    constructor() ERC20("0-protocol USD", "0-USD") {
        _mint(msg.sender, 1000000 * 10**6); // 1 million 0-USD (6 decimals)
    }

    function mint(address to, uint256 amount) public {
        _mint(to, amount);
    }
    
    function decimals() public view virtual override returns (uint8) {
        return 6;
    }
}
