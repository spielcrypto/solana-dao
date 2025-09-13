# Solana DAO with Telegram Bot

A decentralized autonomous organization (DAO) system built on Solana with a Telegram bot interface for easy group management and voting.

## Features

### Solana Program Features
- **Group Management**: Create and manage DAO groups
- **Proposal Creation**: Create proposals with multiple choices
- **Token-Weighted Voting**: Support for both token-weighted and equal voting
- **Time-Based Voting**: Set voting periods for proposals
- **Member Management**: Add/remove group members
- **Event Logging**: All actions are logged as events
- **User Account Management**: Create and manage user accounts linked to Telegram IDs

### Telegram Bot Features
- **Group Integration**: Works in Telegram groups and private chats
- **Admin Controls**: Only group admins can create groups and proposals
- **Automatic Wallet Generation**: Users get unique Solana wallets generated from their Telegram ID
- **Real-time Results**: View proposal results and voting statistics
- **User-Friendly Commands**: Simple commands for all DAO operations
- **Balance Checking**: Check SOL balance for user wallets
- **Account Management**: View account information and wallet details

## Architecture

### Solana Program Structure
```
programs/solana-dao/src/lib.rs
├── DaoRegistry - Global registry of all DAO groups
├── Group - Individual DAO group with proposals and members
├── Proposal - Individual proposal with voting data
├── UserAccount - User account linked to Telegram ID
└── Instructions:
    ├── initialize - Initialize the DAO registry
    ├── create_group - Create a new DAO group
    ├── create_proposal - Create a new proposal
    ├── vote_on_proposal - Vote on a proposal
    ├── add_group_member - Add member to group
    ├── remove_group_member - Remove member from group
    ├── create_user_account - Create user account
    ├── login_user - Login/verify user account
    └── get_all_groups - Retrieve all groups
```

### Bot Structure
```
bot/src/main.rs
├── Command handlers for all bot operations
├── Solana client integration
├── Secure user wallet management (Telegram ID + SECRET_SEED)
├── Admin permission checks
├── Manual deserialization for account data
└── Cryptographic seed generation with environment variables
```

## Setup Instructions

### Prerequisites
- Rust and Cargo installed
- Solana CLI installed
- Anchor framework installed
- Node.js and Yarn installed
- Telegram Bot Token (from @BotFather)

### 1. Deploy Solana Program

```bash
# Build the program
anchor build

# Deploy to localnet (make sure localnet is running)
solana-test-validator # in another terminal
anchor deploy

# For devnet deployment:
# anchor deploy --provider.cluster devnet
```

### 2. Setup Environment Variables

Create a `.env` file in the `bot/` directory:

```env
TELOXIDE_TOKEN=your_telegram_bot_token_here
RUST_LOG=info
SECRET_SEED=your_very_secure_secret_seed_here
```

**Important Security Note**: The `SECRET_SEED` is used to generate secure, unpredictable user wallets. Choose a long, random string (at least 32 characters) and keep it secret. This prevents wallet addresses from being predictable based on Telegram IDs alone.

### 3. Run the Telegram Bot

```bash
cd bot
cargo run
```

### 4. Setup Bot in Telegram

1. Add your bot to a Telegram group
2. Make the bot an admin (optional, but recommended for better functionality)
3. Use `/start` to initialize the bot

## Bot Commands

### Basic Commands
- `/start` - Start the bot and get welcome message
- `/help` - Show all available commands

### Account Management
- `/login` - Create or access your Solana account (automatic wallet generation)
- `/account` - Show your account information and wallet details
- `/balance` - Show your SOL balance

### Group Management (Admin Only)
- `/creategroup "name" "description"` - Create a new DAO group
  - Example: `/creategroup "My DAO" "A DAO for community decisions"`
- `/listgroups` - List all DAO groups

### Proposal Management (Admin Only)
- `/createproposal <title> <description> <choices> <duration_hours>` - Create a new proposal
  - Example: `/createproposal "Budget Allocation" "How should we allocate the budget?" "Marketing,Development,Operations" 48`
- `/listproposals <group_id>` - List proposals for a group

### Voting (All Users)
- `/vote <group_id> <proposal_id> <choice_number>` - Vote on a proposal
  - Example: `/vote group-uuid-here proposal-uuid-here 1` (vote for choice 1)
- `/results <group_id> <proposal_id>` - View proposal results

## Usage Examples

### 1. User Account Setup
```
User: /login
Bot: ✅ Account created successfully!
     👤 Username: username
     🔑 Wallet Address: 9fAcbMc9eBNYnRPe2mRvSFsKDxssVfkQp6wfoT5rjFy6
     📅 Created: 2024-01-15 14:30 UTC
     🔗 View on Explorer: https://explorer.solana.com/address/9fAcbMc9eBNYnRPe2mRvSFsKDxssVfkQp6wfoT5rjFy6?cluster=localnet
     ✅ Account is active and ready for DAO participation!
```

### 2. Check Balance
```
User: /balance
Bot: 💰 Your SOL Balance
     👤 Username: username
     🔑 Wallet Address: 9fAcbMc9eBNYnRPe2mRvSFsKDxssVfkQp6wfoT5rjFy6
     💎 Balance: 0.000000 SOL
     🔗 View on Explorer: https://explorer.solana.com/address/9fAcbMc9eBNYnRPe2mRvSFsKDxssVfkQp6wfoT5rjFy6?cluster=localnet
```

### 3. Creating a DAO Group
```
Admin: /creategroup "My DAO" "A DAO for managing our community decisions"
Bot: ✅ DAO Group created successfully!
     📋 Name: My DAO
     📝 Description: A DAO for managing our community decisions
```

### 4. Listing Groups
```
User: /listgroups
Bot: 📋 DAO Groups:
     1. My DAO
        📝 A DAO for managing our community decisions
```

### 5. Creating a Proposal
```
Admin: /createproposal "Budget Allocation" "How should we allocate the budget?" "Marketing,Development,Operations" 48
Bot: ✅ Proposal created successfully!
     📋 Budget Allocation
     📝 How should we allocate the budget?
     ⏰ Voting ends: 2024-01-17 14:30 UTC
     
     Choices:
     0. Marketing
     1. Development
     2. Operations
```

### 6. Voting on a Proposal
```
User: /vote group-uuid-here proposal-uuid-here 1
Bot: ✅ Vote cast successfully!
     🗳️ Proposal: proposal-uuid-here
     ✔️ Your choice: 1
     👤 Wallet: 9fAcbMc9eBNYnRPe2mRvSFsKDxssVfkQp6wfoT5rjFy6
```

## Technical Details

### Wallet Generation
- Each user gets a unique Solana wallet generated from their Telegram ID + SECRET_SEED
- Wallets are deterministic and can be recreated from the same Telegram ID and secret seed
- Uses cryptographic hashing to ensure wallet addresses are unpredictable and secure
- No need for users to manage private keys or seed phrases
- The SECRET_SEED environment variable adds an extra layer of security

### Account Management
- User accounts are stored on-chain as Program Derived Addresses (PDAs)
- Each account links a Telegram ID to a Solana wallet address
- Account creation is automatic on first `/login` command

### Deserialization
- The bot uses manual deserialization to handle Anchor account data
- Skips 8-byte discriminator and handles zero-padding in allocated accounts
- Robust error handling for corrupted or incomplete account data

## Development

### Program Development
The Solana program is built with Anchor framework. Key files:
- `programs/solana-dao/src/lib.rs` - Main program code
- `programs/solana-dao/Cargo.toml` - Program dependencies

### Bot Development
The Telegram bot is built with Teloxide. Key files:
- `bot/src/main.rs` - Main bot code
- `bot/Cargo.toml` - Bot dependencies

### Testing
```bash
# Test Solana program
anchor test

# Test bot (ensure you have test environment variables set)
cd bot
cargo test
```

## Security Considerations

1. **Admin Verification**: Only Telegram group admins can create groups and proposals
2. **Secure Wallet Generation**: User wallets are generated using Telegram ID + SECRET_SEED with cryptographic hashing
3. **Secret Seed Protection**: The SECRET_SEED environment variable must be kept secret and should be a long, random string
4. **Voting Integrity**: Each user can only vote once per proposal
5. **Time Constraints**: Proposals have defined voting periods
6. **Token Verification**: For token-weighted voting, token balances are verified on-chain
7. **Account Validation**: All user accounts are validated on-chain before operations
8. **Unpredictable Addresses**: Wallet addresses cannot be predicted without knowing both the Telegram ID and SECRET_SEED

## Funding Accounts

### For Development (Localnet)
The bot automatically handles SOL airdrops for the payer account during development.

### For Devnet
Use the Solana CLI to fund accounts:
```bash
# Fund a specific wallet
solana airdrop 2 <WALLET_ADDRESS> --url devnet

# Check balance
solana balance <WALLET_ADDRESS> --url devnet
```

### For Mainnet
Users need to fund their wallets with real SOL from exchanges or other sources.

## Troubleshooting

### Common Issues
1. **Bot not responding**: Check if bot token is correct and bot is added to group
2. **Solana transactions failing**: Ensure you have sufficient SOL for transaction fees
3. **Permission errors**: Make sure bot has admin rights in Telegram group
4. **Account creation fails**: Check if the Solana program is deployed and accessible
5. **Deserialization errors**: The bot handles these automatically with fallback mechanisms

### Logs
Enable detailed logging by setting:
```env
RUST_LOG=debug
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

This project is open source and available under the [MIT License](LICENSE).