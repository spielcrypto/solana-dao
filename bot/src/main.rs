#![allow(deprecated)]
use anchor_client::solana_sdk::signer::Signer;
use anchor_lang::AnchorDeserialize;
use dotenv::dotenv;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use teloxide::types::BotCommand;
use tokio::sync::Mutex;

use anchor_client::solana_sdk::{
    commitment_config::CommitmentConfig, native_token::LAMPORTS_PER_SOL, pubkey::Pubkey,
    signature::Keypair, system_instruction,
};
use anchor_client::{Client, Cluster, Program};
use anchor_lang::system_program;
use chrono::{DateTime, Utc};
use std::str::FromStr;
use teloxide::{prelude::*, utils::command::BotCommands};
use uuid::Uuid;

mod solana_dao {
    use anchor_lang::prelude::*;
    use anchor_lang::AccountDeserialize;

    declare_id!("4mwBvEQbpGJKDDZCvEPTujCefmphw1fZ99Jxhz69oHcT");

    #[derive(AnchorSerialize, AnchorDeserialize, Clone)]
    pub struct GroupInfo {
        pub group_id: String,
        pub authority: Pubkey,
        pub pubkey: Pubkey,
    }

    #[derive(AnchorSerialize, AnchorDeserialize, Clone)]
    pub struct ProposalInfo {
        pub proposal_id: String,
        pub pubkey: Pubkey,
        pub created_at: i64,
    }

    #[derive(AnchorSerialize, AnchorDeserialize, Clone)]
    pub struct GroupMember {
        pub pubkey: Pubkey,
        pub joined_at: i64,
    }

    #[derive(AnchorSerialize, AnchorDeserialize, Clone)]
    pub struct VoterInfo {
        pub voter: Pubkey,
        pub choice: u8,
        pub vote_weight: u64,
        pub timestamp: i64,
    }

    #[derive(AnchorSerialize, AnchorDeserialize, Clone)]
    pub struct DaoRegistry {
        pub authority: Pubkey,
        pub groups: Vec<GroupInfo>,
        pub bump: u8,
    }

    impl AccountDeserialize for DaoRegistry {
        fn try_deserialize_unchecked(buf: &mut &[u8]) -> anchor_lang::Result<Self> {
            AnchorDeserialize::deserialize(buf)
                .map_err(|_| anchor_lang::error::ErrorCode::AccountDidNotDeserialize.into())
        }
    }

    #[derive(AnchorSerialize, AnchorDeserialize, Clone)]
    pub struct Group {
        pub group_id: String,
        pub name: String,
        pub description: String,
        pub authority: Pubkey,
        pub proposals: Vec<ProposalInfo>,
        pub members: Vec<GroupMember>,
        pub created_at: i64,
        pub bump: u8,
    }

    impl AccountDeserialize for Group {
        fn try_deserialize_unchecked(buf: &mut &[u8]) -> anchor_lang::Result<Self> {
            AnchorDeserialize::deserialize(buf)
                .map_err(|_| anchor_lang::error::ErrorCode::AccountDidNotDeserialize.into())
        }
    }

    #[derive(AnchorSerialize, AnchorDeserialize, Clone)]
    pub struct Proposal {
        pub proposal_id: String,
        pub group_id: String,
        pub title: String,
        pub description: String,
        pub choices: Vec<String>,
        pub choice_votes: Vec<u64>,
        pub voting_start: i64,
        pub voting_end: i64,
        pub token_mint: Option<Pubkey>,
        pub creator: Pubkey,
        pub voters: Vec<VoterInfo>,
        pub created_at: i64,
        pub bump: u8,
    }

    impl AccountDeserialize for Proposal {
        fn try_deserialize_unchecked(buf: &mut &[u8]) -> anchor_lang::Result<Self> {
            AnchorDeserialize::deserialize(buf)
                .map_err(|_| anchor_lang::error::ErrorCode::AccountDidNotDeserialize.into())
        }
    }

    #[derive(AnchorSerialize, AnchorDeserialize, Clone)]
    pub struct UserAccount {
        pub telegram_id: i64,
        pub wallet_pubkey: Pubkey,
        pub created_at: i64,
        pub bump: u8,
    }

    impl AccountDeserialize for UserAccount {
        fn try_deserialize_unchecked(buf: &mut &[u8]) -> anchor_lang::Result<Self> {
            AnchorDeserialize::deserialize(buf)
                .map_err(|_| anchor_lang::error::ErrorCode::AccountDidNotDeserialize.into())
        }
    }
}

#[derive(BotCommands, Clone, Debug)]
#[command(rename_rule = "lowercase", description = "Solana DAO Bot Commands")]
enum Command {
    #[command(description = "Display help message")]
    Help,
    #[command(description = "Start the bot")]
    Start,
    #[command(description = "Create a new DAO group")]
    CreateGroup(String), // Combined: "name description"
    #[command(description = "List all DAO groups")]
    ListGroups,
    #[command(description = "Create a new proposal")]
    CreateProposal(String), // Combined: "title description choices duration_hours"
    #[command(description = "List proposals for a group")]
    ListProposals,
    #[command(description = "Vote on a proposal", parse_with = "split")]
    Vote { proposal_id: String, choice: u8 },
    #[command(description = "Get proposal results")]
    Results { proposal_id: String },
    #[command(description = "Create or access your Solana account")]
    Login,
    #[command(description = "Show your account information")]
    Account,
    #[command(description = "Show your SOL balance")]
    Balance,
    #[command(description = "Fund your account with SOL for voting")]
    FundAccount,
}

#[derive(Clone)]
struct BotState {
    solana_client: Arc<anchor_client::Client<Arc<Keypair>>>,
    program: Arc<Program<Arc<Keypair>>>,
    payer: Arc<Keypair>,
    user_seeds: Arc<Mutex<HashMap<UserId, [u8; 32]>>>, // telegram_id -> seed for keypair generation
    admin_groups: Arc<Mutex<HashMap<i64, String>>>,    // chat_id -> group_id
}

impl BotState {
    async fn new() -> anyhow::Result<Self> {
        // Load or create keypair for the bot payer
        let payer = Arc::new(load_or_create_payer_keypair().await?);

        let client = Client::new_with_options(
            Cluster::Localnet, // Change to Devnet/Mainnet as needed
            payer.clone(),
            CommitmentConfig::processed(),
        );

        let program = client.program(solana_dao::ID)?;

        // Ensure the payer has some SOL for transactions
        let _ = ensure_payer_funded(&client, &payer).await;

        // Initialize the DAO registry if it doesn't exist (ignore errors if already initialized)
        match initialize_dao_registry(&client, &program, &payer).await {
            Ok(result) => {
                if result != "already_initialized" {
                    log::info!("DAO registry initialized: {}", result);
                }
            }
            Err(e) => {
                log::warn!(
                    "DAO registry initialization failed (may already exist): {}",
                    e
                );
            }
        }

        Ok(Self {
            solana_client: Arc::new(client),
            program: Arc::new(program),
            payer,
            user_seeds: Arc::new(Mutex::new(HashMap::new())),
            admin_groups: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

async fn answer(bot: Bot, msg: Message, cmd: Command, state: BotState) -> ResponseResult<()> {
    log::info!("Command received: {:?}", cmd);
    match cmd {
        Command::Help => {
            bot.send_message(msg.chat.id, Command::descriptions().to_string())
                .await?;
        }
        Command::Start => {
            let welcome_msg = "Welcome to Solana DAO Bot! 🚀\n\n\
                Use /help to see available commands.\n\
                Use /login to create or access your account automatically.\n\
                Use /account to view your wallet address and account info.";
            bot.send_message(msg.chat.id, welcome_msg).await?;
        }
        Command::CreateGroup(args) => {
            // Parse the arguments: "name description" or "name" "description"
            let (name, description) = if args.contains('"') {
                // Handle quoted arguments
                let mut parts = Vec::new();
                let mut current = String::new();
                let mut in_quotes = false;

                for c in args.chars() {
                    match c {
                        '"' => in_quotes = !in_quotes,
                        ' ' if !in_quotes => {
                            if !current.trim().is_empty() {
                                parts.push(current.trim().to_string());
                                current.clear();
                            }
                        }
                        _ => current.push(c),
                    }
                }
                if !current.trim().is_empty() {
                    parts.push(current.trim().to_string());
                }

                if parts.len() >= 2 {
                    (parts[0].clone(), parts[1].clone())
                } else {
                    (String::new(), String::new())
                }
            } else {
                // Handle space-separated arguments
                let parts: Vec<&str> = args.splitn(2, ' ').collect();
                if parts.len() >= 2 {
                    (parts[0].to_string(), parts[1].to_string())
                } else {
                    (parts.get(0).unwrap_or(&"").to_string(), String::new())
                }
            };

            if name.is_empty() || description.is_empty() {
                bot.send_message(msg.chat.id, "Usage: /creategroup <name> <description>\nExample: /creategroup \"My Group\" \"Group description\"").await?;
                return Ok(());
            }

            handle_create_group(bot, msg, name, description, state).await?;
        }
        Command::ListGroups => {
            handle_list_groups(bot, msg, state).await?;
        }
        Command::CreateProposal(args) => {
            // Parse the arguments: "title description choices duration_hours"
            let (title, description, choices, duration_hours) = if args.contains('"') {
                // Handle quoted arguments
                let mut parts = Vec::new();
                let mut current = String::new();
                let mut in_quotes = false;

                for c in args.chars() {
                    match c {
                        '"' => in_quotes = !in_quotes,
                        ' ' if !in_quotes => {
                            if !current.trim().is_empty() {
                                parts.push(current.trim().to_string());
                                current.clear();
                            }
                        }
                        _ => current.push(c),
                    }
                }
                if !current.trim().is_empty() {
                    parts.push(current.trim().to_string());
                }

                if parts.len() >= 4 {
                    let duration_str = parts[3].clone();
                    let duration_hours = duration_str.parse::<u32>().unwrap_or(24);
                    (
                        parts[0].clone(),
                        parts[1].clone(),
                        parts[2].clone(),
                        duration_hours,
                    )
                } else {
                    (String::new(), String::new(), String::new(), 24)
                }
            } else {
                // Handle space-separated arguments
                let parts: Vec<&str> = args.splitn(4, ' ').collect();
                if parts.len() >= 4 {
                    let duration_hours = parts[3].parse::<u32>().unwrap_or(24);
                    (
                        parts[0].to_string(),
                        parts[1].to_string(),
                        parts[2].to_string(),
                        duration_hours,
                    )
                } else {
                    (String::new(), String::new(), String::new(), 24)
                }
            };

            if title.is_empty() || description.is_empty() || choices.is_empty() {
                bot.send_message(msg.chat.id, "Usage: /createproposal <title> <description> <choices> <duration_hours>\nExample: /createproposal \"Budget Allocation\" \"How should we allocate the budget?\" \"Marketing,Development,Operations\" 48").await?;
                return Ok(());
            }

            handle_create_proposal(bot, msg, title, description, choices, duration_hours, state)
                .await?;
        }
        Command::ListProposals => {
            handle_list_proposals(bot, msg, state).await?;
        }
        Command::Vote {
            proposal_id,
            choice,
        } => {
            handle_vote(bot, msg, proposal_id, choice, state).await?;
        }
        Command::Results { proposal_id } => {
            handle_results(bot, msg, proposal_id, state).await?;
        }
        Command::Login => {
            handle_login(bot, msg, state).await?;
        }
        Command::Account => {
            handle_account(bot, msg, state).await?;
        }
        Command::Balance => {
            handle_balance(bot, msg, state).await?;
        }
        Command::FundAccount => {
            handle_fund_account(bot, msg, state).await?;
        }
    }
    Ok(())
}

async fn handle_fund_account(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    let user_id = match msg.from() {
        Some(user) => user.id,
        None => {
            bot.send_message(msg.chat.id, "❌ Unable to identify user. Please try again.")
                .await?;
            return Ok(());
        }
    };
    let telegram_id = user_id.0 as i64;

    // Ensure user has an account
    let user_keypair = match ensure_user_account(&state, telegram_id).await {
        Ok(keypair) => keypair,
        Err(e) => {
            bot.send_message(
                msg.chat.id,
                format!(
                    "❌ Failed to access your account: {}. Please try /login first.",
                    e
                ),
            )
            .await?;
            return Ok(());
        }
    };

    // Check current balance
    let program = match state.solana_client.program(solana_dao::ID) {
        Ok(program) => program,
        Err(e) => {
            log::error!("Failed to get program: {}", e);
            bot.send_message(
                msg.chat.id,
                "❌ Failed to access Solana program. Please try again later.",
            )
            .await?;
            return Ok(());
        }
    };

    let balance = match program.rpc().get_balance(&user_keypair.pubkey()).await {
        Ok(balance) => balance,
        Err(e) => {
            log::error!("Failed to get balance: {}", e);
            bot.send_message(
                msg.chat.id,
                "❌ Failed to check account balance. Please try again later.",
            )
            .await?;
            return Ok(());
        }
    };
    let balance_sol = balance as f64 / 1_000_000_000.0; // Convert lamports to SOL

    if balance > 10_000_000 {
        // More than 0.01 SOL
        bot.send_message(
            msg.chat.id,
            format!(
                "✅ Your account already has sufficient SOL balance!\n\n\
                💰 Current balance: {:.6} SOL\n\
                💡 You can vote on proposals now!",
                balance_sol
            ),
        )
        .await?;
        return Ok(());
    }

    // Fund the account with 0.01 SOL
    let fund_instruction = system_instruction::transfer(
        &state.payer.pubkey(),
        &user_keypair.pubkey(),
        10_000_000, // 0.01 SOL
    );

    let recent_blockhash = match program.rpc().get_latest_blockhash().await {
        Ok(blockhash) => blockhash,
        Err(e) => {
            log::error!("Failed to get blockhash: {}", e);
            bot.send_message(
                msg.chat.id,
                "❌ Failed to get recent blockhash. Please try again later.",
            )
            .await?;
            return Ok(());
        }
    };

    let fund_transaction =
        anchor_client::solana_sdk::transaction::Transaction::new_signed_with_payer(
            &[fund_instruction],
            Some(&state.payer.pubkey()),
            &[&state.payer],
            recent_blockhash,
        );

    match program
        .rpc()
        .send_and_confirm_transaction(&fund_transaction)
        .await
    {
        Ok(signature) => {
            let new_balance = match program.rpc().get_balance(&user_keypair.pubkey()).await {
                Ok(balance) => balance,
                Err(e) => {
                    log::error!("Failed to get new balance: {}", e);
                    bot.send_message(
                        msg.chat.id,
                        "❌ Failed to check new balance. Please try /balance to verify.",
                    )
                    .await?;
                    return Ok(());
                }
            };
            let new_balance_sol = new_balance as f64 / 1_000_000_000.0;

            bot.send_message(
                msg.chat.id,
                format!(
                    "✅ <b>Account funded successfully!</b>\n\n\
                    💰 New balance: {:.6} SOL\n\
                    🔗 Transaction: https://explorer.solana.com/tx/{}?cluster=localnet\n\n\
                    💡 You can now vote on proposals!",
                    new_balance_sol, signature
                ),
            )
            .parse_mode(teloxide::types::ParseMode::Html)
            .await?;
        }
        Err(e) => {
            bot.send_message(
                msg.chat.id,
                format!(
                    "❌ Failed to fund account: {}\n\n\
                    Please try again later or contact support.",
                    e
                ),
            )
            .await?;
        }
    }

    Ok(())
}

async fn handle_create_group(
    bot: Bot,
    msg: Message,
    name: String,
    description: String,
    state: BotState,
) -> ResponseResult<()> {
    log::info!(
        "handle_create_group called with name: '{}', description: '{}'",
        name,
        description
    );
    // Only allow group admins to create DAO groups
    match is_chat_admin(&bot, &msg).await {
        Ok(is_admin) => {
            if !is_admin {
                bot.send_message(msg.chat.id, "Only group admins can create DAO groups.")
                    .await?;
                return Ok(());
            }
        }
        Err(e) => {
            bot.send_message(msg.chat.id, format!("Error checking admin status: {}", e))
                .await?;
            return Ok(());
        }
    }

    let group_id = format!("tg_{}", msg.chat.id.0.abs());

    let group_name = msg.chat.first_name().unwrap_or("Anonymous").to_string();

    // Store the admin group mapping
    {
        let mut admin_groups = state.admin_groups.lock().await;
        admin_groups.insert(msg.chat.id.0, group_id.clone());
    }

    // Try to create the group on Solana
    match create_solana_group(&state, &group_id, &name, &description).await {
        Ok(signature) => {
            let response = format!(
                "✅ DAO Group created successfully!\n\n\
                📋 Name: {}\n\
                📝 Description: {}\n\
                🆔 Group name: {}\n\
                🔗 Transaction: https://explorer.solana.com/tx/{}?cluster=localnet",
                name, description, group_name, signature
            );
            bot.send_message(msg.chat.id, response).await?;
        }
        Err(e) => {
            log::error!("Failed to create DAO group '{}': {}", name, e);
            let error_str = e.to_string();
            let user_msg = if error_str.contains("already in use")
                || error_str.contains("AlreadyInUse")
                || error_str.contains("Allocate: account")
            {
                "❌ A DAO group with this ID already exists in this chat."
            } else {
                "❌ Failed to create DAO group. Please try again later or contact support."
            };
            bot.send_message(msg.chat.id, user_msg).await?;
        }
    }

    Ok(())
}

async fn handle_list_groups(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    match get_all_groups(&state).await {
        Ok(groups) => {
            if groups.is_empty() {
                bot.send_message(msg.chat.id, "No DAO groups found.")
                    .await?;
            } else {
                let mut response = "📋 <b>DAO Groups:</b>\n\n".to_string();
                for (i, group) in groups.iter().enumerate() {
                    response.push_str(&format!(
                        "{}. <b>{}</b>\n   📝 {}\n\n",
                        i + 1,
                        group.name,
                        group.description
                    ));
                }
                bot.send_message(msg.chat.id, response)
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .await?;
            }
        }
        Err(e) => {
            log::error!("Failed to fetch groups: {}", e);
            let error_str = e.to_string();
            let user_msg = if error_str.contains("AccountDidNotDeserialize") {
                "❌ No groups found or groups data is corrupted. Try creating a new group first."
            } else {
                "❌ Failed to fetch groups. Please try again later."
            };
            bot.send_message(msg.chat.id, user_msg).await?;
        }
    }
    Ok(())
}

async fn handle_create_proposal(
    bot: Bot,
    msg: Message,
    title: String,
    description: String,
    choices: String,
    duration_hours: u32,
    state: BotState,
) -> ResponseResult<()> {
    // Only allow group admins to create proposals
    match is_chat_admin(&bot, &msg).await {
        Ok(is_admin) => {
            if !is_admin {
                bot.send_message(msg.chat.id, "Only group admins can create proposals.")
                    .await?;
                return Ok(());
            }
        }
        Err(e) => {
            bot.send_message(msg.chat.id, format!("Error checking admin status: {}", e))
                .await?;
            return Ok(());
        }
    }

    let choices_vec: Vec<String> = choices
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if choices_vec.len() < 2 || choices_vec.len() > 10 {
        bot.send_message(
            msg.chat.id,
            "Please provide between 2 and 10 choices, separated by commas.",
        )
        .await?;
        return Ok(());
    }

    let proposal_id = Uuid::new_v4().to_string();
    log::info!(
        "Generated proposal_id: {} (length: {})",
        proposal_id,
        proposal_id.len()
    );
    let now = Utc::now();
    let voting_start = now.timestamp();
    let voting_end = (now + chrono::Duration::hours(duration_hours as i64)).timestamp();

    let group_id = format!("tg_{}", msg.chat.id.0.abs());

    match create_solana_proposal(
        &state,
        &group_id,
        &proposal_id,
        &title,
        &description,
        choices_vec.clone(),
        voting_start,
        voting_end,
    )
    .await
    {
        Ok(signature) => {
            let choices_text = choices_vec
                .iter()
                .enumerate()
                .map(|(i, choice)| format!("{}. {}", i, choice))
                .collect::<Vec<_>>()
                .join("\n");

            let response = format!(
                "✅ <b>Proposal created successfully!</b>\n\n\
                📋 <b>{}</b>\n\
                📝 {}\n\
                🆔 <b>Proposal ID:</b> <code>{}</code>\n\
                ⏰ <b>Voting ends:</b> {}\n\n\
                <b>Choices:</b>\n{}\n\n\
                🔗 <a href=\"https://explorer.solana.com/tx/{}?cluster=localnet\">View Transaction</a>\n\n\
                Use <code>/vote {} &lt;choice_number&gt;</code> to vote!",
                title,
                description,
                proposal_id,
                DateTime::<Utc>::from_timestamp(voting_end, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                    .unwrap_or_else(|| "Unknown time".to_string()),
                choices_text,
                signature,
                proposal_id
            );
            bot.send_message(msg.chat.id, response)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await?;
        }
        Err(e) => {
            let error_msg = format!("❌ Failed to create proposal: {}", e);
            bot.send_message(msg.chat.id, error_msg).await?;
        }
    }

    Ok(())
}

async fn handle_list_proposals(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    let group_id = format!("tg_{}", msg.chat.id.0.abs());
    match get_group_proposals(&state, &group_id).await {
        Ok(proposals) => {
            if proposals.is_empty() {
                bot.send_message(msg.chat.id, "No proposals found for this group.")
                    .await?;
            } else {
                let mut response = "📋 <b>Proposals:</b>\n\n".to_string();
                for (i, proposal) in proposals.iter().enumerate() {
                    let status = if Utc::now().timestamp() > proposal.voting_end {
                        "🔒 Ended"
                    } else if Utc::now().timestamp() < proposal.voting_start {
                        "⏳ Not started"
                    } else {
                        "🗳️ Active"
                    };

                    // Format choices for display
                    let choices_text = proposal
                        .choices
                        .iter()
                        .enumerate()
                        .map(|(idx, choice)| format!("{}. {}", idx, choice))
                        .collect::<Vec<_>>()
                        .join("\n      ");

                    response.push_str(&format!(
                        "{}. <b>{}</b> {}\n   📝 {}\n   🗳️ <b>Choices:</b>\n      {}\n   🆔 <b>ID:</b> <code>{}</code>\n   ⏰ <b>Ends:</b> {}\n\n",
                        i + 1,
                        proposal.title,
                        status,
                        proposal.description,
                        choices_text,
                        proposal.proposal_id,
                        DateTime::<Utc>::from_timestamp(proposal.voting_end, 0)
                            .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
                            .unwrap_or_else(|| "Unknown time".to_string())
                    ));
                }
                bot.send_message(msg.chat.id, response)
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .await?;
            }
        }
        Err(e) => {
            let error_msg = format!("❌ Failed to fetch proposals: {}", e);
            bot.send_message(msg.chat.id, error_msg).await?;
        }
    }
    Ok(())
}

async fn handle_vote(
    bot: Bot,
    msg: Message,
    proposal_id: String,
    choice: u8,
    state: BotState,
) -> ResponseResult<()> {
    let user_id = match msg.from() {
        Some(user) => user.id,
        None => {
            bot.send_message(msg.chat.id, "❌ Unable to identify user. Please try again.")
                .await?;
            return Ok(());
        }
    };
    let telegram_id = user_id.0 as i64;
    let group_id = format!("tg_{}", msg.chat.id.0.abs());

    // Ensure user has an account
    let user_keypair = match ensure_user_account(&state, telegram_id).await {
        Ok(keypair) => keypair,
        Err(e) => {
            bot.send_message(
                msg.chat.id,
                format!(
                    "❌ Failed to access your account: {}. Please try /login first.",
                    e
                ),
            )
            .await?;
            return Ok(());
        }
    };

    match vote_on_proposal(
        &state,
        &group_id,
        &proposal_id,
        choice,
        user_keypair.pubkey(),
    )
    .await
    {
        Ok(signature) => {
            let response = format!(
                "✅ Vote cast successfully!\n\n\
                🗳️ Proposal: {}\n\
                ✔️ Your choice: {}\n\
                👤 Wallet: {}\n\
                🔗 Transaction: https://explorer.solana.com/tx/{}?cluster=localnet",
                proposal_id,
                choice,
                user_keypair.pubkey(),
                signature
            );
            bot.send_message(msg.chat.id, response).await?;
        }
        Err(e) => {
            let error_str = e.to_string();
            let user_msg = if error_str.contains("AlreadyVoted") || error_str.contains("0x177a") {
                "❌ You have already voted on this proposal!\n\n\
                🗳️ Each user can only vote once per proposal.\n\
                💡 Use /results to see the current results."
                    .to_string()
            } else if error_str.contains("VotingNotActive") {
                "❌ Voting is not currently active for this proposal.\n\n\
                ⏰ The voting period may have ended or not started yet.\n\
                💡 Use /results to check the proposal status."
                    .to_string()
            } else if error_str.contains("InvalidChoice") {
                "❌ Invalid choice selected!\n\n\
                🗳️ Please select a valid choice number for this proposal.\n\
                💡 Use /listproposals to see available choices."
                    .to_string()
            } else if error_str.contains("You don't have enough SOL balance") {
                "❌ Insufficient SOL balance!\n\n\
                💰 You need at least 0.001 SOL for transaction fees.\n\
                💡 Use /fundaccount to add SOL to your account."
                    .to_string()
            } else {
                format!("❌ Failed to vote: {}", e)
            };
            bot.send_message(msg.chat.id, user_msg).await?;
        }
    }

    Ok(())
}

async fn handle_results(
    bot: Bot,
    msg: Message,
    proposal_id: String,
    state: BotState,
) -> ResponseResult<()> {
    let group_id = format!("tg_{}", msg.chat.id.0.abs());
    match get_proposal_results(&state, &group_id, &proposal_id).await {
        Ok(proposal) => {
            let total_votes: u64 = proposal.choice_votes.iter().sum();

            let mut response = format!(
                "📊 <b>Results for: {}</b>\n\n\
                📝 {}\n\
                🗳️ Total votes: {}\n\
                👥 Total voters: {}\n\n\
                <b>Results:</b>\n",
                html_escape(&proposal.title),
                html_escape(&proposal.description),
                total_votes,
                proposal.voters.len()
            );

            for (i, (choice, votes)) in proposal
                .choices
                .iter()
                .zip(proposal.choice_votes.iter())
                .enumerate()
            {
                let percentage = if total_votes > 0 {
                    (*votes as f64 / total_votes as f64) * 100.0
                } else {
                    0.0
                };
                response.push_str(&format!(
                    "{}. {} - {} votes ({:.1}%)\n",
                    i,
                    html_escape(choice),
                    votes,
                    percentage
                ));
            }

            let status = if Utc::now().timestamp() > proposal.voting_end {
                "🔒 Voting has ended"
            } else {
                "🗳️ Voting is still active"
            };
            response.push_str(&format!("\n{}", status));

            bot.send_message(msg.chat.id, response)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await?;
        }
        Err(e) => {
            let error_msg = format!("❌ Failed to get results: {}", e);
            bot.send_message(msg.chat.id, error_msg).await?;
        }
    }
    Ok(())
}

// Helper function to escape HTML special characters
fn html_escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            '\'' => "&#x27;".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

async fn handle_login(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    let user_id = match msg.from() {
        Some(user) => user.id,
        None => {
            bot.send_message(msg.chat.id, "❌ Unable to identify user. Please try again.")
                .await?;
            return Ok(());
        }
    };
    let telegram_id = user_id.0 as i64;
    let user = msg.from();

    let username = user.and_then(|u| u.username.as_ref());

    match create_user_account(&state, telegram_id).await {
        Ok(keypair) => {
            let response = format!(
                "✅ Account ready!\n\n\
                👤 Telegram username: {}\n\
                🔑 Wallet Address: {}\n\n\
                You can now participate in DAO voting!",
                username
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "anonymous".to_string()),
                keypair.pubkey()
            );
            bot.send_message(msg.chat.id, response).await?;
        }
        Err(e) => {
            let error_msg = format!("❌ Failed to create/access account: {}", e);
            bot.send_message(msg.chat.id, error_msg).await?;
        }
    }

    Ok(())
}

async fn handle_account(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    let user_id = match msg.from() {
        Some(user) => user.id,
        None => {
            bot.send_message(msg.chat.id, "❌ Unable to identify user. Please try again.")
                .await?;
            return Ok(());
        }
    };
    let telegram_id = user_id.0 as i64;

    let user = msg.from();
    let username = user.and_then(|u| u.username.as_ref());

    // Check if user has an account
    let user_seeds = state.user_seeds.lock().await;
    let seed_opt = user_seeds.get(&user_id).copied();
    drop(user_seeds);

    match seed_opt {
        Some(seed) => {
            let keypair = Keypair::new_from_array(seed);
            let wallet_address = keypair.pubkey();

            // Try to get account info from Solana
            let (user_account_pda, _) = Pubkey::find_program_address(
                &[b"user_account", telegram_id.to_le_bytes().as_ref()],
                &solana_dao::ID,
            );

            match state
                .program
                .account::<solana_dao::UserAccount>(user_account_pda)
                .await
            {
                Ok(user_account) => {
                    let created_date = if user_account.created_at == 0 {
                        "Just created".to_string()
                    } else {
                        match chrono::DateTime::<chrono::Utc>::from_timestamp(
                            user_account.created_at,
                            0,
                        ) {
                            Some(dt) => dt.format("%Y-%m-%d %H:%M UTC").to_string(),
                            None => "Recently created".to_string(),
                        }
                    };

                    let response = format!(
                        "👤 <b>Your Account Information</b>\n\n\
                        👤 Username: <code>{}</code>\n\
                        🔑 Wallet Address: <code>{}</code>\n\
                        📅 Created: {}\n\
                        🔗 View on Explorer: https://explorer.solana.com/address/{}?cluster=localnet\n\n\
                        ✅ Account is active and ready for DAO participation!",
                        username.map(|s| s.to_string()).unwrap_or_else(|| "anonymous".to_string()),
                        wallet_address,
                        created_date,
                        wallet_address
                    );

                    bot.send_message(msg.chat.id, response)
                        .parse_mode(teloxide::types::ParseMode::Html)
                        .await?;
                }
                Err(_) => {
                    let response = format!(
                        "⚠️ <b>Account Found Locally</b>\n\n\
                        👤 Username: <code>{}</code>\n\
                        🔑 Wallet Address: <code>{}</code>\n\
                        🔗 View on Explorer: https://explorer.solana.com/address/{}?cluster=localnet\n\n\
                        ❌ Account not yet created on-chain. Use /login to create it.",
                        username.map(|s| s.to_string()).unwrap_or_else(|| "anonymous".to_string()),
                        wallet_address,
                        wallet_address
                    );

                    bot.send_message(msg.chat.id, response)
                        .parse_mode(teloxide::types::ParseMode::Html)
                        .await?;
                }
            }
        }
        None => {
            // User doesn't have an account yet
            bot.send_message(
                msg.chat.id,
                "❌ You don't have an account yet. Use /login to create one.",
            )
            .parse_mode(teloxide::types::ParseMode::Html)
            .await?;
        }
    }

    Ok(())
}

async fn handle_balance(bot: Bot, msg: Message, state: BotState) -> ResponseResult<()> {
    let user_id = match msg.from() {
        Some(user) => user.id,
        None => {
            bot.send_message(msg.chat.id, "❌ Unable to identify user. Please try again.")
                .await?;
            return Ok(());
        }
    };

    let user = msg.from();
    let username = user.and_then(|u| u.username.clone());

    // Check if user has a seed (account exists)
    if let Some(seed) = state.user_seeds.lock().await.get(&user_id) {
        // Generate the same keypair from the seed
        let keypair = Keypair::new_from_array(*seed);
        let wallet_address = keypair.pubkey();

        // Get the balance from Solana
        match state.program.rpc().get_balance(&wallet_address).await {
            Ok(balance_lamports) => {
                let balance_sol = balance_lamports as f64 / LAMPORTS_PER_SOL as f64;

                let response = format!(
                    "💰 <b>Your SOL Balance</b>\n\n\
                    👤 Username: <code>{}</code>\n\
                    🔑 Wallet Address: <code>{}</code>\n\
                    💎 Balance: <b>{:.6} SOL</b>\n\
                    🔗 View on Explorer: https://explorer.solana.com/address/{}?cluster=localnet",
                    username.unwrap_or_else(|| "anonymous".to_string()),
                    wallet_address,
                    balance_sol,
                    wallet_address
                );

                bot.send_message(msg.chat.id, response)
                    .parse_mode(teloxide::types::ParseMode::Html)
                    .await?;
            }
            Err(e) => {
                log::error!("Failed to get balance: {:?}", e);
                bot.send_message(
                    msg.chat.id,
                    "❌ Failed to get balance. Please try again later.",
                )
                .parse_mode(teloxide::types::ParseMode::Html)
                .await?;
            }
        }
    } else {
        // User doesn't have an account yet
        bot.send_message(
            msg.chat.id,
            "❌ You don't have an account yet. Use /login to create one.",
        )
        .parse_mode(teloxide::types::ParseMode::Html)
        .await?;
    }

    Ok(())
}

// Helper function to ensure user has an account, creating one if needed
async fn ensure_user_account(state: &BotState, telegram_id: i64) -> anyhow::Result<Keypair> {
    // Check if we already have a seed for this user
    {
        let user_seeds = state.user_seeds.lock().await;
        if let Some(seed) = user_seeds.get(&UserId(telegram_id as u64)) {
            return Ok(Keypair::new_from_array(*seed));
        }
    }

    // Try to get existing account from Solana
    let (user_account_pda, _) = Pubkey::find_program_address(
        &[b"user_account", telegram_id.to_le_bytes().as_ref()],
        &solana_dao::ID,
    );

    match state
        .program
        .account::<solana_dao::UserAccount>(user_account_pda)
        .await
    {
        Ok(_user_account) => {
            // Account exists, we need to generate/retrieve the keypair
            // In a production system, you'd want to securely store and retrieve the private key
            // For this demo, we'll generate a deterministic keypair based on telegram_id
            let seed = generate_seed_from_telegram_id(telegram_id);
            let keypair = Keypair::new_from_array(seed);

            // Store the seed for future use
            {
                let mut user_seeds = state.user_seeds.lock().await;
                user_seeds.insert(UserId(telegram_id as u64), seed);
            }

            Ok(keypair)
        }
        Err(_) => {
            // Account doesn't exist, create it
            create_user_account(state, telegram_id).await
        }
    }
}

// Generate a deterministic seed from telegram ID and secret seed
// Uses SECRET_SEED environment variable for additional security
fn generate_seed_from_telegram_id(telegram_id: i64) -> [u8; 32] {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Get the secret seed from environment variable
    let secret_seed = std::env::var("SECRET_SEED")
        .unwrap_or_else(|_| "default_secret_seed_change_this_in_production".to_string());

    // Create a hash of telegram_id + secret_seed for cryptographic security
    let mut hasher = DefaultHasher::new();
    telegram_id.hash(&mut hasher);
    secret_seed.hash(&mut hasher);
    let hash = hasher.finish();

    // Convert hash to 32-byte seed
    let mut seed = [0u8; 32];
    let hash_bytes = hash.to_le_bytes();

    // Use the hash as the base and fill the rest with additional entropy
    for i in 0..8 {
        seed[i] = hash_bytes[i];
    }

    // Add additional entropy by mixing telegram_id and secret_seed
    let id_bytes = telegram_id.to_le_bytes();
    for i in 8..16 {
        seed[i] = id_bytes[i - 8] ^ secret_seed.as_bytes()[(i - 8) % secret_seed.len()];
    }

    // Fill remaining bytes with deterministic but secure pattern
    for i in 16..32 {
        seed[i] = (hash_bytes[i % 8]
            ^ id_bytes[i % 8]
            ^ secret_seed.as_bytes()[i % secret_seed.len()]) as u8;
    }

    log::info!("Generated secure seed for telegram_id: {}", telegram_id);
    seed
}

// Create a new user account on Solana
async fn create_user_account(state: &BotState, telegram_id: i64) -> anyhow::Result<Keypair> {
    let seed = generate_seed_from_telegram_id(telegram_id);

    // Create keypair from seed using the correct method
    let keypair = Keypair::new_from_array(seed);

    log::info!("Keypair created successfully: {}", keypair.pubkey());

    // Get the user account PDA
    let (user_account_pda, _) = Pubkey::find_program_address(
        &[b"user_account", telegram_id.to_le_bytes().as_ref()],
        &solana_dao::ID,
    );

    log::info!("Creating user account for telegram_id: {}", telegram_id);
    log::info!("User wallet pubkey: {}", keypair.pubkey());
    log::info!("User account PDA: {}", user_account_pda);
    log::info!("Payer pubkey: {}", state.payer.pubkey());

    // Check if account already exists
    let program = state.solana_client.program(solana_dao::ID)?;
    let rpc_client = program.rpc();

    match rpc_client.get_account(&user_account_pda).await {
        Ok(_account) => {
            log::info!("User account already exists, returning existing keypair");
            // Store the seed for future use if not already stored
            {
                let mut user_seeds = state.user_seeds.lock().await;
                user_seeds.insert(UserId(telegram_id as u64), seed);
            }
            return Ok(keypair);
        }
        Err(_) => {
            log::info!("User account does not exist, creating new one");
        }
    }

    // Build the transaction manually but with proper error handling
    log::info!("Building transaction manually...");

    // Build instruction data for create_user_account using correct discriminator
    let mut instruction_data = vec![146, 68, 100, 69, 63, 46, 182, 199]; // create_user_account discriminator from IDL
    instruction_data.extend_from_slice(&telegram_id.to_le_bytes());

    log::info!("Instruction data: {:?}", instruction_data);
    log::info!("Telegram ID bytes: {:?}", telegram_id.to_le_bytes());

    let accounts = vec![
        anchor_client::solana_sdk::instruction::AccountMeta::new(user_account_pda, false),
        anchor_client::solana_sdk::instruction::AccountMeta::new_readonly(
            keypair.pubkey(),
            false, // user_wallet is not a signer according to IDL
        ),
        anchor_client::solana_sdk::instruction::AccountMeta::new(state.payer.pubkey(), true),
        anchor_client::solana_sdk::instruction::AccountMeta::new_readonly(
            system_program::ID,
            false,
        ),
    ];

    log::info!("Instruction accounts:");
    for (i, account) in accounts.iter().enumerate() {
        log::info!(
            "  {}: {} (writable: {}, signer: {})",
            i,
            account.pubkey,
            account.is_writable,
            account.is_signer
        );
    }

    let instruction = anchor_client::solana_sdk::instruction::Instruction {
        program_id: solana_dao::ID,
        accounts,
        data: instruction_data,
    };

    // Use the program's RPC client for better compatibility
    let program = state.solana_client.program(solana_dao::ID)?;
    let rpc_client = program.rpc();

    log::info!("Getting recent blockhash...");
    let recent_blockhash = rpc_client.get_latest_blockhash().await?;
    log::info!("Recent blockhash: {}", recent_blockhash);

    log::info!("Creating transaction...");
    let mut transaction = anchor_client::solana_sdk::transaction::Transaction::new_with_payer(
        &[instruction],
        Some(&state.payer.pubkey()),
    );
    transaction.sign(&[&state.payer], recent_blockhash);

    log::info!("Transaction created, attempting to send...");
    log::info!("Transaction signatures: {:?}", transaction.signatures);

    match rpc_client.send_and_confirm_transaction(&transaction).await {
        Ok(signature) => {
            log::info!("Transaction successful: {}", signature);
        }
        Err(e) => {
            log::error!("Transaction failed: {}", e);
            return Err(e.into());
        }
    }

    // Store the seed for future use
    {
        let mut user_seeds = state.user_seeds.lock().await;
        user_seeds.insert(UserId(telegram_id as u64), seed);
    }

    Ok(keypair)
}

// Initialize the DAO registry
async fn initialize_dao_registry(
    client: &Client<Arc<Keypair>>,
    program: &Program<Arc<Keypair>>,
    payer: &Arc<Keypair>,
) -> anyhow::Result<String> {
    // Get the DAO registry PDA
    let (dao_registry_pda, _) = Pubkey::find_program_address(&[b"dao_registry"], &solana_dao::ID);

    println!("Init - Program ID: {}", solana_dao::ID);
    println!("Init - DAO Registry PDA: {}", dao_registry_pda);

    // Check if already initialized
    if let Ok(_) = program
        .account::<solana_dao::DaoRegistry>(dao_registry_pda)
        .await
    {
        return Ok("already_initialized".to_string());
    }

    // Build initialize instruction using correct discriminator
    let instruction_data = vec![175, 175, 109, 31, 13, 152, 155, 237]; // initialize discriminator from IDL

    let instruction = anchor_client::solana_sdk::instruction::Instruction {
        program_id: solana_dao::ID,
        accounts: vec![
            anchor_client::solana_sdk::instruction::AccountMeta::new(dao_registry_pda, false),
            anchor_client::solana_sdk::instruction::AccountMeta::new(payer.pubkey(), true),
            anchor_client::solana_sdk::instruction::AccountMeta::new_readonly(
                system_program::ID,
                false,
            ),
        ],
        data: instruction_data,
    };

    let program_id = instruction.program_id.clone();

    let recent_blockhash = client
        .program(program_id)?
        .rpc()
        .get_latest_blockhash()
        .await?;
    let transaction = anchor_client::solana_sdk::transaction::Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[&**payer],
        recent_blockhash,
    );
    let tx = client
        .program(program_id)?
        .rpc()
        .send_and_confirm_transaction(&transaction)
        .await?;

    Ok(tx.to_string())
}

// Helper functions for Solana interactions
async fn create_solana_group(
    state: &BotState,
    group_id: &str,
    name: &str,
    description: &str,
) -> anyhow::Result<String> {
    // Get the DAO registry PDA
    let (dao_registry_pda, _) = Pubkey::find_program_address(&[b"dao_registry"], &solana_dao::ID);

    // Get the group PDA
    let (group_pda, _) =
        Pubkey::find_program_address(&[b"group", group_id.as_bytes()], &solana_dao::ID);

    // Build instruction data using correct discriminator
    let mut instruction_data = vec![79, 60, 158, 134, 61, 199, 56, 248]; // create_group discriminator from IDL
    instruction_data.extend_from_slice(&(group_id.len() as u32).to_le_bytes());
    instruction_data.extend_from_slice(group_id.as_bytes());
    instruction_data.extend_from_slice(&(name.len() as u32).to_le_bytes());
    instruction_data.extend_from_slice(name.as_bytes());
    instruction_data.extend_from_slice(&(description.len() as u32).to_le_bytes());
    instruction_data.extend_from_slice(description.as_bytes());

    let instruction = anchor_client::solana_sdk::instruction::Instruction {
        program_id: solana_dao::ID,
        accounts: vec![
            anchor_client::solana_sdk::instruction::AccountMeta::new(group_pda, false),
            anchor_client::solana_sdk::instruction::AccountMeta::new(dao_registry_pda, false),
            anchor_client::solana_sdk::instruction::AccountMeta::new(state.payer.pubkey(), true),
            anchor_client::solana_sdk::instruction::AccountMeta::new_readonly(
                system_program::ID,
                false,
            ),
        ],
        data: instruction_data,
    };

    let program_id = instruction.program_id.clone();

    let recent_blockhash = state
        .solana_client
        .program(program_id)?
        .rpc()
        .get_latest_blockhash()
        .await?;
    let transaction = anchor_client::solana_sdk::transaction::Transaction::new_signed_with_payer(
        &[instruction],
        Some(&state.payer.pubkey()),
        &[&state.payer],
        recent_blockhash,
    );
    let tx = state
        .solana_client
        .program(program_id)?
        .rpc()
        .send_and_confirm_transaction(&transaction)
        .await?;

    Ok(tx.to_string())
}

async fn get_all_groups(state: &BotState) -> anyhow::Result<Vec<solana_dao::Group>> {
    // Get the DAO registry PDA
    let (dao_registry_pda, _) = Pubkey::find_program_address(&[b"dao_registry"], &solana_dao::ID);

    println!("DAO Registry PDA: {}", dao_registry_pda);
    println!("Program ID used: {}", solana_dao::ID);

    // First check if the account exists
    match state.program.rpc().get_account(&dao_registry_pda).await {
        Ok(account) => {
            log::info!(
                "DAO registry account exists with {} bytes",
                account.data.len()
            );
        }
        Err(e) => {
            log::error!("DAO registry account does not exist or error: {}", e);
            return Ok(Vec::new());
        }
    }

    // Try to fetch and deserialize the DAO registry account manually
    match state.program.rpc().get_account(&dao_registry_pda).await {
        Ok(account) => {
            log::info!("Account data length: {} bytes", account.data.len());

            if account.data.len() < 8 {
                log::error!("Account data too short: {} bytes", account.data.len());
                return Ok(Vec::new());
            }

            // Skip the 8-byte discriminator and deserialize manually
            let data = &account.data[8..];

            // Find the actual data length by looking for the end of meaningful data
            // The account is padded with zeros, so we need to find where the real data ends
            let mut actual_data_len = data.len();
            for (i, &byte) in data.iter().enumerate().rev() {
                if byte != 0 {
                    actual_data_len = i + 1;
                    break;
                }
            }

            log::info!(
                "Actual data length: {} bytes (out of {} total)",
                actual_data_len,
                data.len()
            );

            // Only deserialize the actual data portion
            let actual_data = &data[..actual_data_len];

            // Deserialize the DaoRegistry struct manually using Anchor
            match solana_dao::DaoRegistry::try_from_slice(actual_data) {
                Ok(dao_registry) => {
                    log::info!(
                        "Successfully deserialized DAO registry with {} groups",
                        dao_registry.groups.len()
                    );

                    // Fetch all group accounts
                    let mut groups = Vec::new();
                    for group_info in dao_registry.groups {
                        log::info!(
                            "Attempting to fetch group: {} with pubkey: {}",
                            group_info.group_id,
                            group_info.pubkey
                        );
                        // Try to fetch group account manually (same approach as DAO registry)
                        match state.program.rpc().get_account(&group_info.pubkey).await {
                            Ok(account) => {
                                log::info!(
                                    "Group account exists with {} bytes",
                                    account.data.len()
                                );

                                if account.data.len() < 8 {
                                    log::error!(
                                        "Group account data too short: {} bytes",
                                        account.data.len()
                                    );
                                    continue;
                                }

                                // Skip the 8-byte discriminator
                                let data = &account.data[8..];

                                // Find the actual data length by looking for the end of meaningful data
                                let mut actual_data_len = data.len();
                                for (i, &byte) in data.iter().enumerate().rev() {
                                    if byte != 0 {
                                        actual_data_len = i + 1;
                                        break;
                                    }
                                }

                                log::info!(
                                    "Group actual data length: {} bytes (out of {} total)",
                                    actual_data_len,
                                    data.len()
                                );

                                // Only deserialize the actual data portion
                                let actual_data = &data[..actual_data_len];

                                match solana_dao::Group::try_from_slice(actual_data) {
                                    Ok(group) => {
                                        log::info!("Successfully fetched group: {}", group.name);
                                        groups.push(group);
                                    }
                                    Err(e) => {
                                        log::error!(
                                            "Failed to deserialize group {}: {}",
                                            group_info.group_id,
                                            e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!(
                                    "Failed to get group account {}: {}",
                                    group_info.group_id,
                                    e
                                );
                            }
                        }
                    }
                    Ok(groups)
                }
                Err(e) => {
                    log::error!("Manual deserialization failed: {}", e);
                    log::info!("Returning empty groups list due to deserialization error");
                    Ok(Vec::new())
                }
            }
        }
        Err(e) => {
            log::error!("Failed to get account: {}", e);
            Ok(Vec::new())
        }
    }
}

async fn create_solana_proposal(
    state: &BotState,
    group_id: &str,
    proposal_id: &str,
    title: &str,
    description: &str,
    choices: Vec<String>,
    voting_start: i64,
    voting_end: i64,
) -> anyhow::Result<String> {
    // Get the group PDA
    let (group_pda, _) =
        Pubkey::find_program_address(&[b"group", group_id.as_bytes()], &solana_dao::ID);

    log::info!("Group PDA: {}", group_pda);
    log::info!("Current payer (authority): {}", state.payer.pubkey());
    log::info!("Looking for group with ID: '{}'", group_id);

    // Check if group exists and get its authority
    let program = state.solana_client.program(solana_dao::ID)?;

    // First, let's check if the account exists at all
    match program.rpc().get_account(&group_pda).await {
        Ok(account) => {
            log::info!("Group account exists with {} bytes", account.data.len());
        }
        Err(e) => {
            log::error!("Group account does not exist: {}", e);
            return Err(anyhow::anyhow!(
                "Group '{}' does not exist. Please create the group first.",
                group_id
            ));
        }
    }

    match program.account::<solana_dao::Group>(group_pda).await {
        Ok(group) => {
            log::info!("Group found - Authority: {}", group.authority);
            log::info!(
                "Group name: '{}', description: '{}'",
                group.name,
                group.description
            );
            if group.authority != state.payer.pubkey() {
                return Err(anyhow::anyhow!(
                    "Unauthorized: Group authority ({}) does not match current payer ({})",
                    group.authority,
                    state.payer.pubkey()
                ));
            }
        }
        Err(e) => {
            log::error!("Failed to deserialize group account: {}", e);

            // Try manual deserialization like in get_all_groups
            match program.rpc().get_account(&group_pda).await {
                Ok(account) => {
                    log::info!("Attempting manual deserialization...");
                    if account.data.len() < 8 {
                        log::error!("Account data too short: {} bytes", account.data.len());
                        return Err(anyhow::anyhow!("Group '{}' data is corrupted.", group_id));
                    }

                    let data = &account.data[8..];
                    let mut actual_data_len = data.len();
                    for (i, &byte) in data.iter().enumerate().rev() {
                        if byte != 0 {
                            actual_data_len = i + 1;
                            break;
                        }
                    }

                    log::info!(
                        "Manual deserialization - actual data length: {} bytes",
                        actual_data_len
                    );
                    let actual_data = &data[..actual_data_len];

                    match solana_dao::Group::try_from_slice(actual_data) {
                        Ok(group) => {
                            log::info!(
                                "Manual deserialization successful - Group: '{}', Authority: {}",
                                group.name,
                                group.authority
                            );
                            if group.authority != state.payer.pubkey() {
                                return Err(anyhow::anyhow!(
                                    "Unauthorized: Group authority ({}) does not match current payer ({})",
                                    group.authority,
                                    state.payer.pubkey()
                                ));
                            }
                        }
                        Err(deser_err) => {
                            log::error!("Manual deserialization also failed: {}", deser_err);
                            return Err(anyhow::anyhow!("Group '{}' exists but data is corrupted and cannot be deserialized.", group_id));
                        }
                    }
                }
                Err(acc_err) => {
                    log::error!(
                        "Failed to get account for manual deserialization: {}",
                        acc_err
                    );
                    return Err(anyhow::anyhow!(
                        "Group '{}' does not exist. Please create the group first.",
                        group_id
                    ));
                }
            }
        }
    }

    // Get the proposal PDA
    // Use first 8 bytes of group_pda and proposal_id to stay within 32-byte seed limit (8 + 8 + 8 = 24 bytes)
    let (proposal_pda, _) = Pubkey::find_program_address(
        &[
            b"proposal",
            &group_pda.to_bytes()[..8],
            &proposal_id.as_bytes()[..8],
        ],
        &solana_dao::ID,
    );

    log::info!("Proposal PDA: {}", proposal_pda);

    // Build instruction data using correct discriminator
    let mut instruction_data = vec![132, 116, 68, 174, 216, 160, 198, 22]; // create_proposal discriminator from IDL
    instruction_data.extend_from_slice(&(proposal_id.len() as u32).to_le_bytes());
    instruction_data.extend_from_slice(proposal_id.as_bytes());
    instruction_data.extend_from_slice(&(title.len() as u32).to_le_bytes());
    instruction_data.extend_from_slice(title.as_bytes());
    instruction_data.extend_from_slice(&(description.len() as u32).to_le_bytes());
    instruction_data.extend_from_slice(description.as_bytes());
    instruction_data.extend_from_slice(&(choices.len() as u32).to_le_bytes());
    for choice in &choices {
        instruction_data.extend_from_slice(&(choice.len() as u32).to_le_bytes());
        instruction_data.extend_from_slice(choice.as_bytes());
    }
    instruction_data.extend_from_slice(&voting_start.to_le_bytes());
    instruction_data.extend_from_slice(&voting_end.to_le_bytes());
    // Use NATIVE_MINT for SOL-weighted voting
    instruction_data.push(1); // Some for token_mint
                              // Native SOL mint address: So11111111111111111111111111111111111111112
    let native_mint = match Pubkey::from_str("So11111111111111111111111111111111111111112") {
        Ok(pubkey) => pubkey,
        Err(e) => {
            log::error!("Failed to parse native mint address: {}", e);
            return Err(anyhow::anyhow!(
                "Failed to parse native mint address: {}",
                e
            ));
        }
    };
    instruction_data.extend_from_slice(&native_mint.to_bytes());

    let instruction = anchor_client::solana_sdk::instruction::Instruction {
        program_id: solana_dao::ID,
        accounts: vec![
            anchor_client::solana_sdk::instruction::AccountMeta::new(proposal_pda, false),
            anchor_client::solana_sdk::instruction::AccountMeta::new(group_pda, false),
            anchor_client::solana_sdk::instruction::AccountMeta::new(state.payer.pubkey(), true),
            anchor_client::solana_sdk::instruction::AccountMeta::new_readonly(
                system_program::ID,
                false,
            ),
        ],
        data: instruction_data,
    };

    let program_id = instruction.program_id.clone();
    let recent_blockhash = state
        .solana_client
        .program(program_id)?
        .rpc()
        .get_latest_blockhash()
        .await?;
    let transaction = anchor_client::solana_sdk::transaction::Transaction::new_signed_with_payer(
        &[instruction],
        Some(&state.payer.pubkey()),
        &[&state.payer],
        recent_blockhash,
    );
    let tx = state
        .solana_client
        .program(program_id)?
        .rpc()
        .send_and_confirm_transaction(&transaction)
        .await?;

    Ok(tx.to_string())
}

async fn get_group_proposals(
    state: &BotState,
    group_id: &str,
) -> anyhow::Result<Vec<solana_dao::Proposal>> {
    // Get the group PDA
    let (group_pda, _) =
        Pubkey::find_program_address(&[b"group", group_id.as_bytes()], &solana_dao::ID);

    // Fetch the group account manually (same approach as get_all_groups)
    let group = match state.program.rpc().get_account(&group_pda).await {
        Ok(account) => {
            if account.data.len() < 8 {
                return Err(anyhow::anyhow!(
                    "Group account data too short: {} bytes",
                    account.data.len()
                ));
            }

            // Skip the 8-byte discriminator
            let data = &account.data[8..];

            // Find the actual data length by looking for the end of meaningful data
            let mut actual_data_len = data.len();
            for (i, &byte) in data.iter().enumerate().rev() {
                if byte != 0 {
                    actual_data_len = i + 1;
                    break;
                }
            }

            // Only deserialize the actual data portion
            let actual_data = &data[..actual_data_len];

            match solana_dao::Group::try_from_slice(actual_data) {
                Ok(group) => group,
                Err(e) => {
                    log::error!("Failed to deserialize group {}: {}", group_id, e);
                    return Err(anyhow::anyhow!("Failed to deserialize group: {}", e));
                }
            }
        }
        Err(e) => {
            log::error!("Failed to get group account {}: {}", group_id, e);
            return Err(anyhow::anyhow!("Failed to get group account: {}", e));
        }
    };

    // Fetch all proposal accounts manually (same approach as groups)
    let mut proposals = Vec::new();
    for proposal_info in group.proposals {
        match state.program.rpc().get_account(&proposal_info.pubkey).await {
            Ok(account) => {
                if account.data.len() < 8 {
                    log::error!(
                        "Proposal account data too short: {} bytes",
                        account.data.len()
                    );
                    continue;
                }

                // Skip the 8-byte discriminator
                let data = &account.data[8..];

                // Find the actual data length by looking for the end of meaningful data
                let mut actual_data_len = data.len();
                for (i, &byte) in data.iter().enumerate().rev() {
                    if byte != 0 {
                        actual_data_len = i + 1;
                        break;
                    }
                }

                // Only deserialize the actual data portion
                let actual_data = &data[..actual_data_len];

                match solana_dao::Proposal::try_from_slice(actual_data) {
                    Ok(proposal) => {
                        log::info!("Successfully fetched proposal: {}", proposal.title);
                        proposals.push(proposal);
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to deserialize proposal {}: {}",
                            proposal_info.proposal_id,
                            e
                        );
                    }
                }
            }
            Err(e) => {
                log::error!(
                    "Failed to get proposal account {}: {}",
                    proposal_info.proposal_id,
                    e
                );
            }
        }
    }

    Ok(proposals)
}

async fn vote_on_proposal(
    state: &BotState,
    group_id: &str,
    proposal_id: &str,
    choice: u8,
    voter_wallet: Pubkey,
) -> anyhow::Result<String> {
    // Get the group PDA
    let (group_pda, _) =
        Pubkey::find_program_address(&[b"group", group_id.as_bytes()], &solana_dao::ID);

    log::info!("Group PDA: {}", group_pda);

    // Get the proposal PDA - use first 8 bytes of group_pda and proposal_id to stay within 32-byte seed limit
    let (proposal_pda, _) = Pubkey::find_program_address(
        &[
            b"proposal",
            &group_pda.to_bytes()[..8],
            &proposal_id.as_bytes()[..8],
        ],
        &solana_dao::ID,
    );

    log::info!("Proposal PDA: {}", proposal_pda);

    // Find the user's seed and generate keypair
    let voter_keypair = {
        let user_seeds = state.user_seeds.lock().await;
        let seed = user_seeds
            .values()
            .find(|seed| {
                let kp = Keypair::new_from_array(**seed);
                kp.pubkey() == voter_wallet
            })
            .copied()
            .ok_or_else(|| anyhow::anyhow!("User seed not found"))?;
        Keypair::new_from_array(seed)
    };

    log::info!("Voter Keypair: {}", voter_keypair.pubkey());

    // Check if user has enough SOL balance for transaction fees
    let program = state.solana_client.program(solana_dao::ID)?;
    let balance = program.rpc().get_balance(&voter_wallet).await?;
    log::info!("User SOL balance: {} lamports", balance);

    if balance < 5000 {
        // Less than 0.000005 SOL (minimum for transaction fees)
        return Err(anyhow::anyhow!(
            "You don't have enough SOL balance to vote. Please fund your account with at least 0.001 SOL for transaction fees."
        ));
    }

    // For SOL-weighted voting, we can use simple placeholders since the program
    // uses ctx.accounts.voter.lamports() directly and doesn't validate the token accounts
    let instruction = anchor_client::solana_sdk::instruction::Instruction {
        program_id: solana_dao::ID,
        accounts: vec![
            anchor_client::solana_sdk::instruction::AccountMeta::new(proposal_pda, false),
            anchor_client::solana_sdk::instruction::AccountMeta::new(voter_wallet, true),
            // voter_token_account - use voter wallet as placeholder (not validated for SOL voting)
            anchor_client::solana_sdk::instruction::AccountMeta::new_readonly(
                voter_wallet, // Use voter wallet as placeholder
                false,
            ),
            // token_program - use system program as placeholder (not validated for SOL voting)
            anchor_client::solana_sdk::instruction::AccountMeta::new_readonly(
                system_program::ID, // Use system program as placeholder
                false,
            ),
        ],
        data: vec![188, 239, 13, 88, 119, 199, 251, 119, choice], // discriminator + choice
    };

    let program_id = instruction.program_id.clone();
    log::info!(
        "Created instruction with {} accounts",
        instruction.accounts.len()
    );

    let recent_blockhash = state
        .solana_client
        .program(program_id)?
        .rpc()
        .get_latest_blockhash()
        .await?;
    log::info!("Got recent blockhash: {}", recent_blockhash);

    let transaction = anchor_client::solana_sdk::transaction::Transaction::new_signed_with_payer(
        &[instruction],
        Some(&voter_wallet),
        &[&voter_keypair],
        recent_blockhash,
    );
    log::info!("Created transaction, sending...");

    let tx = state
        .solana_client
        .program(program_id)?
        .rpc()
        .send_and_confirm_transaction(&transaction)
        .await?;

    log::info!("Transaction sent successfully: {}", tx);
    Ok(tx.to_string())
}

async fn get_proposal_results(
    state: &BotState,
    group_id: &str,
    proposal_id: &str,
) -> anyhow::Result<solana_dao::Proposal> {
    // Get the group PDA
    let (group_pda, _) =
        Pubkey::find_program_address(&[b"group", group_id.as_bytes()], &solana_dao::ID);

    // Get the proposal PDA
    // Use first 8 bytes of group_pda and proposal_id to stay within 32-byte seed limit (8 + 8 + 8 = 24 bytes)
    let (proposal_pda, _) = Pubkey::find_program_address(
        &[
            b"proposal",
            &group_pda.to_bytes()[..8],
            &proposal_id.as_bytes()[..8],
        ],
        &solana_dao::ID,
    );

    log::info!("Fetching proposal results for PDA: {}", proposal_pda);

    // Fetch the proposal account manually (same approach as get_group_proposals)
    match state.program.rpc().get_account(&proposal_pda).await {
        Ok(account) => {
            if account.data.len() < 8 {
                return Err(anyhow::anyhow!(
                    "Proposal account data too short: {} bytes",
                    account.data.len()
                ));
            }

            // Skip the 8-byte discriminator
            let data = &account.data[8..];

            // Find the actual data length by looking for the end of meaningful data
            let mut actual_data_len = data.len();
            for (i, &byte) in data.iter().enumerate().rev() {
                if byte != 0 {
                    actual_data_len = i + 1;
                    break;
                }
            }

            log::info!(
                "Proposal actual data length: {} bytes (out of {} total)",
                actual_data_len,
                data.len()
            );

            // Only deserialize the actual data portion
            let actual_data = &data[..actual_data_len];

            match solana_dao::Proposal::try_from_slice(actual_data) {
                Ok(proposal) => {
                    log::info!("Successfully fetched proposal: {}", proposal.title);
                    Ok(proposal)
                }
                Err(e) => {
                    log::error!("Failed to deserialize proposal {}: {}", proposal_id, e);
                    Err(anyhow::anyhow!("Failed to deserialize proposal: {}", e))
                }
            }
        }
        Err(e) => {
            log::error!("Failed to get proposal account {}: {}", proposal_id, e);
            Err(anyhow::anyhow!("Failed to get proposal account: {}", e))
        }
    }
}

async fn is_chat_admin(bot: &Bot, msg: &Message) -> anyhow::Result<bool> {
    if msg.chat.is_private() {
        return Ok(true); // In private chats, user is always "admin"
    }

    let user_id = match msg.from() {
        Some(user) => user.id,
        None => {
            return Err(anyhow::anyhow!("Unable to identify user"));
        }
    };
    let chat_member = bot
        .get_chat_member(msg.chat.id, user_id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get chat member: {}", e))?;

    Ok(matches!(
        chat_member.kind,
        teloxide::types::ChatMemberKind::Owner(_)
            | teloxide::types::ChatMemberKind::Administrator(_)
    ))
}

// Load existing payer keypair or create a new one
async fn load_or_create_payer_keypair() -> anyhow::Result<Keypair> {
    let keypair_path = "bot/bot-payer-keypair.json";

    if Path::new(keypair_path).exists() {
        // Load existing keypair
        let keypair_data = fs::read_to_string(keypair_path)?;
        let keypair_bytes: Vec<u8> = serde_json::from_str(&keypair_data)?;
        Ok(Keypair::try_from(&keypair_bytes[..])?)
    } else {
        // Create new keypair and save it
        let keypair = Keypair::new();
        let keypair_bytes = keypair.to_bytes().to_vec();
        let keypair_data = serde_json::to_string(&keypair_bytes)?;
        fs::write(keypair_path, keypair_data)?;
        log::info!("Created new payer keypair at: {}", keypair_path);
        log::info!("Payer pubkey: {}", keypair.pubkey());
        Ok(keypair)
    }
}

// Ensure the payer account has enough SOL for transactions
async fn ensure_payer_funded(
    client: &Client<Arc<Keypair>>,
    payer: &Arc<Keypair>,
) -> anyhow::Result<()> {
    // Create a program instance to access RPC
    let program = client.program(solana_dao::ID)?;
    let rpc_client = program.rpc();

    let balance = rpc_client.get_balance(&payer.pubkey()).await?;
    let min_balance = LAMPORTS_PER_SOL / 10; // 0.1 SOL minimum

    if balance < min_balance {
        log::info!(
            "Payer balance too low ({} lamports), requesting airdrop...",
            balance
        );

        // Request airdrop (this works on localnet/devnet)
        let airdrop_amount = LAMPORTS_PER_SOL; // 1 SOL
        let signature = rpc_client
            .request_airdrop(&payer.pubkey(), airdrop_amount)
            .await?;

        // Wait for confirmation with retries
        log::info!("Waiting for airdrop confirmation...");
        rpc_client.confirm_transaction(&signature).await?;

        // Give it a moment to process
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        let new_balance = rpc_client.get_balance(&payer.pubkey()).await?;
        log::info!("Airdrop successful! New balance: {} lamports", new_balance);

        if new_balance < min_balance {
            return Err(anyhow::anyhow!(
                "Airdrop failed: balance still too low after airdrop"
            ));
        }
    } else {
        log::info!("Payer balance: {} lamports (sufficient)", balance);
    }

    Ok(())
}

async fn message_handler(bot: Bot, msg: Message) -> ResponseResult<()> {
    log::info!("Received message: {:?}", msg.text());
    if let Some(text) = msg.text() {
        if text.starts_with("/login") {
            log::info!("Login command detected, but not processed by command handler");
            bot.send_message(msg.chat.id, "🤖 Bot is working! Login command detected but there might be an issue with command processing.").await?;
        } else {
            log::info!("Non-command message: {}", text);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    pretty_env_logger::init();
    log::info!("Starting Solana DAO Bot...");

    let bot = Bot::from_env();

    let state = match BotState::new().await {
        Ok(state) => state,
        Err(e) => {
            log::error!("Failed to initialize bot state: {}", e);
            return;
        }
    };

    let commands = vec![
        BotCommand::new("help", "Display help message"),
        BotCommand::new("start", "Start the bot"),
        BotCommand::new("creategroup", "Create a new DAO group"),
        BotCommand::new("listgroups", "List all DAO groups"),
        BotCommand::new("createproposal", "Create a new proposal"),
        BotCommand::new("listproposals", "List proposals for a group"),
        BotCommand::new("vote", "Vote on a proposal"),
        BotCommand::new("results", "Get proposal results"),
        BotCommand::new("login", "Create or access your Solana account"),
        BotCommand::new("account", "Show your account information"),
        BotCommand::new("balance", "Show your SOL balance"),
        BotCommand::new("fundaccount", "Fund your account with SOL for voting"),
    ];

    if let Err(e) = bot.set_my_commands(commands).await {
        log::error!("Failed to set bot commands: {}", e);
        // Continue execution even if command setting fails
    }

    Dispatcher::builder(
        bot,
        Update::filter_message()
            .branch(dptree::entry().filter_command::<Command>().endpoint(answer))
            .branch(dptree::endpoint(message_handler)),
    )
    .dependencies(dptree::deps![state])
    .enable_ctrlc_handler()
    .build()
    .dispatch()
    .await;
}
