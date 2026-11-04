const hre = require("hardhat");

async function main() {
  console.log("=================================================");
  console.log("   Deploying Jinn Network Token ($JINN) System    ");
  console.log("=================================================");

  const [deployer] = await hre.ethers.getSigners();
  console.log("Deployer account:", deployer.address);
  const balance = await hre.ethers.provider.getBalance(deployer.address);
  console.log("Account balance:", hre.ethers.formatEther(balance), "BNB\n");

  const treasuryAddress = process.env.TREASURY_SAFE_ADDRESS || deployer.address;
  console.log("Target Treasury / Multi-Sig Receiver:", treasuryAddress);

  // 1. Deploy JinnToken
  console.log("\n[1/3] Deploying JinnToken...");
  const JinnToken = await hre.ethers.getContractFactory("JinnToken");
  const jinnToken = await JinnToken.deploy(treasuryAddress);
  await jinnToken.waitForDeployment();
  const jinnTokenAddress = await jinnToken.getAddress();
  console.log("-> JinnToken deployed at:", jinnTokenAddress);
  console.log("   Total Supply:", (await jinnToken.totalSupply()).toString(), "(1,000,000,000 $JINN)");

  // 2. Deploy JinnVesting
  console.log("\n[2/3] Deploying JinnVesting Vault...");
  const JinnVesting = await hre.ethers.getContractFactory("JinnVesting");
  const jinnVesting = await JinnVesting.deploy(jinnTokenAddress);
  await jinnVesting.waitForDeployment();
  const jinnVestingAddress = await jinnVesting.getAddress();
  console.log("-> JinnVesting deployed at:", jinnVestingAddress);

  // 3. Deploy JinnRewardDistributor
  console.log("\n[3/3] Deploying JinnRewardDistributor...");
  const JinnRewardDistributor = await hre.ethers.getContractFactory("JinnRewardDistributor");
  const distributor = await JinnRewardDistributor.deploy(jinnTokenAddress);
  await distributor.waitForDeployment();
  const distributorAddress = await distributor.getAddress();
  console.log("-> JinnRewardDistributor deployed at:", distributorAddress);

  console.log("\n=================================================");
  console.log("           Deployment Summary                    ");
  console.log("=================================================");
  console.log("Network:                 ", hre.network.name);
  console.log("JinnToken:               ", jinnTokenAddress);
  console.log("JinnVesting:             ", jinnVestingAddress);
  console.log("JinnRewardDistributor:   ", distributorAddress);
  console.log("=================================================");
  console.log("\nNext Steps:");
  console.log("1. Verify contracts on BscScan:");
  console.log(`   npx hardhat verify --network ${hre.network.name} ${jinnTokenAddress} "${treasuryAddress}"`);
  console.log(`   npx hardhat verify --network ${hre.network.name} ${jinnVestingAddress} "${jinnTokenAddress}"`);
  console.log(`   npx hardhat verify --network ${hre.network.name} ${distributorAddress} "${jinnTokenAddress}"`);
  console.log("2. Fund JinnVesting and create Core/Treasury schedules.");
  console.log("3. Deposit PoUC community rewards into JinnRewardDistributor.");
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });
