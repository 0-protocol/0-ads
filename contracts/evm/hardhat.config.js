require("@nomicfoundation/hardhat-toolbox");
require("dotenv").config();

/** @type import('hardhat/config').HardhatUserConfig */
module.exports = {
  solidity: {
    version: "0.8.24",
    settings: {
      evmVersion: "cancun"
    }
  },
  networks: {
    hardhat: {},
    base_sepolia: {
      url: process.env.BASE_SEPOLIA_URL || "https://sepolia.base.org",
      accounts: process.env.PRIVATE_KEY !== undefined ? [process.env.PRIVATE_KEY] : [],
    }
  }
};
