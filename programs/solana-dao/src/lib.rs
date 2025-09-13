#![allow(clippy::result_large_err)]
#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use anchor_lang::prelude::*;
use std::str::FromStr;

declare_id!("4mwBvEQbpGJKDDZCvEPTujCefmphw1fZ99Jxhz69oHcT");

#[program]
pub mod solana_dao {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let dao_registry = &mut ctx.accounts.dao_registry;
        dao_registry.authority = ctx.accounts.authority.key();
        dao_registry.groups = Vec::new();
        dao_registry.bump = ctx.bumps.dao_registry;

        msg!(
            "DAO Registry initialized by: {:?}",
            ctx.accounts.authority.key()
        );
        Ok(())
    }

    pub fn create_group(
        ctx: Context<CreateGroup>,
        group_id: String,
        name: String,
        description: String,
    ) -> Result<()> {
        require!(group_id.len() <= 50, DaoError::GroupIdTooLong);
        require!(name.len() <= 100, DaoError::NameTooLong);
        require!(description.len() <= 500, DaoError::DescriptionTooLong);

        let group = &mut ctx.accounts.group;
        group.group_id = group_id.clone();
        group.name = name;
        group.description = description;
        group.authority = ctx.accounts.authority.key();
        group.proposals = Vec::new();
        group.members = Vec::new();
        group.created_at = Clock::get()?.unix_timestamp;
        group.bump = ctx.bumps.group;

        // Add to registry
        let dao_registry = &mut ctx.accounts.dao_registry;
        dao_registry.groups.push(GroupInfo {
            group_id: group_id.clone(),
            authority: ctx.accounts.authority.key(),
            pubkey: group.key(),
        });

        emit!(GroupCreatedEvent {
            group_id,
            authority: ctx.accounts.authority.key(),
            group_pubkey: group.key(),
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn create_proposal(
        ctx: Context<CreateProposal>,
        proposal_id: String,
        title: String,
        description: String,
        choices: Vec<String>,
        voting_start: i64,
        voting_end: i64,
        token_mint: Option<Pubkey>,
    ) -> Result<()> {
        require!(proposal_id.len() <= 50, DaoError::ProposalIdTooLong);
        require!(title.len() <= 200, DaoError::TitleTooLong);
        require!(description.len() <= 1000, DaoError::DescriptionTooLong);
        require!(
            choices.len() >= 2 && choices.len() <= 10,
            DaoError::InvalidChoiceCount
        );
        require!(voting_start < voting_end, DaoError::InvalidVotingPeriod);
        require!(
            voting_start > Clock::get()?.unix_timestamp,
            DaoError::VotingStartInPast
        );

        let proposal = &mut ctx.accounts.proposal;
        proposal.proposal_id = proposal_id.clone();
        proposal.group_id = ctx.accounts.group.group_id.clone();
        proposal.title = title;
        proposal.description = description;
        proposal.choices = choices.clone();
        proposal.choice_votes = vec![0u64; choices.len()];
        proposal.voting_start = voting_start;
        proposal.voting_end = voting_end;
        proposal.token_mint = token_mint;
        proposal.creator = ctx.accounts.authority.key();
        proposal.voters = Vec::new();
        proposal.created_at = Clock::get()?.unix_timestamp;
        proposal.bump = ctx.bumps.proposal;

        // Add to group
        let group = &mut ctx.accounts.group;
        group.proposals.push(ProposalInfo {
            proposal_id: proposal_id.clone(),
            pubkey: proposal.key(),
            created_at: Clock::get()?.unix_timestamp,
        });

        emit!(ProposalCreatedEvent {
            group_id: group.group_id.clone(),
            proposal_id,
            creator: ctx.accounts.authority.key(),
            proposal_pubkey: proposal.key(),
            voting_start,
            voting_end,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn vote_on_proposal(ctx: Context<VoteOnProposal>, choice_index: u8) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;
        let current_time = Clock::get()?.unix_timestamp;

        require!(
            current_time >= proposal.voting_start && current_time <= proposal.voting_end,
            DaoError::VotingNotActive
        );

        require!(
            (choice_index as usize) < proposal.choices.len(),
            DaoError::InvalidChoice
        );

        // Check if user already voted
        let voter_key = ctx.accounts.voter.key();
        require!(
            !proposal.voters.iter().any(|v| v.voter == voter_key),
            DaoError::AlreadyVoted
        );

        let vote_weight = if let Some(token_mint) = proposal.token_mint {
            if token_mint
                == Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap()
            {
                // SOL-weighted voting
                let voter_balance = ctx.accounts.voter.lamports();
                voter_balance
            } else {
                // SPL Token-weighted voting
                require!(
                    ctx.accounts.voter_token_account.is_some(),
                    DaoError::TokenAccountRequired
                );
                // For SPL token voting, we would need to deserialize the token account
                // For now, return 1 as a placeholder since we're focusing on SOL voting
                1u64
            }
        } else {
            // One person, one vote
            1u64
        };

        require!(vote_weight > 0, DaoError::NoVotingPower);

        // Record the vote
        proposal.choice_votes[choice_index as usize] += vote_weight;
        proposal.voters.push(VoterInfo {
            voter: voter_key,
            choice: choice_index,
            vote_weight,
            timestamp: current_time,
        });

        emit!(VoteCastEvent {
            group_id: proposal.group_id.clone(),
            proposal_id: proposal.proposal_id.clone(),
            voter: voter_key,
            choice: choice_index,
            vote_weight,
            timestamp: current_time,
        });

        Ok(())
    }

    pub fn add_group_member(ctx: Context<AddGroupMember>, member: Pubkey) -> Result<()> {
        let group = &mut ctx.accounts.group;

        require!(
            !group.members.iter().any(|m| m.pubkey == member),
            DaoError::MemberAlreadyExists
        );

        group.members.push(GroupMember {
            pubkey: member,
            joined_at: Clock::get()?.unix_timestamp,
        });

        emit!(MemberAddedEvent {
            group_id: group.group_id.clone(),
            member,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn remove_group_member(ctx: Context<RemoveGroupMember>, member: Pubkey) -> Result<()> {
        let group = &mut ctx.accounts.group;

        let member_index = group
            .members
            .iter()
            .position(|m| m.pubkey == member)
            .ok_or(DaoError::MemberNotFound)?;

        group.members.remove(member_index);

        emit!(MemberRemovedEvent {
            group_id: group.group_id.clone(),
            member,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn create_user_account(ctx: Context<CreateUserAccount>, telegram_id: i64) -> Result<()> {
        let user_account = &mut ctx.accounts.user_account;
        user_account.telegram_id = telegram_id;
        user_account.wallet_pubkey = ctx.accounts.user_wallet.key();
        user_account.created_at = Clock::get()?.unix_timestamp;
        user_account.bump = ctx.bumps.user_account;

        emit!(UserAccountCreatedEvent {
            telegram_id,
            wallet_pubkey: ctx.accounts.user_wallet.key(),
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn login_user(ctx: Context<LoginUser>, telegram_id: i64) -> Result<()> {
        // This function can be used to verify/retrieve existing user account
        let user_account = &ctx.accounts.user_account;

        require!(
            user_account.telegram_id == telegram_id,
            DaoError::InvalidTelegramId
        );

        emit!(UserLoginEvent {
            telegram_id,
            wallet_pubkey: user_account.wallet_pubkey,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    pub fn get_all_groups(ctx: Context<GetAllGroups>) -> Result<()> {
        // This function just returns the DAO registry account
        // The client will deserialize it to get the groups
        let dao_registry = &ctx.accounts.dao_registry;

        msg!("DAO Registry has {} groups", dao_registry.groups.len());

        Ok(())
    }
}

// Account Structs
#[account]
pub struct DaoRegistry {
    pub authority: Pubkey,
    pub groups: Vec<GroupInfo>,
    pub bump: u8,
}

#[account]
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

#[account]
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

#[account]
pub struct UserAccount {
    pub telegram_id: i64,
    pub wallet_pubkey: Pubkey,
    pub created_at: i64,
    pub bump: u8,
}

// Helper Structs
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

// Context Structs
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 4 + (20 * (4 + 50 + 32 + 32)) + 1, // discriminator + authority + vec length + (max 20 groups * (4 + 50 char max group_id + 2 pubkeys)) + bump
        seeds = [b"dao_registry"],
        bump
    )]
    pub dao_registry: Account<'info, DaoRegistry>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(group_id: String)]
pub struct CreateGroup<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 4 + 50 + 4 + 100 + 4 + 500 + 32 + 4 + 4 + 8 + 1, // discriminator + string lengths + data + vecs + bump
        seeds = [b"group", group_id.as_bytes()],
        bump
    )]
    pub group: Account<'info, Group>,

    #[account(mut)]
    pub dao_registry: Account<'info, DaoRegistry>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(proposal_id: String)]
pub struct CreateProposal<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + 4 + 50 + 4 + 50 + 4 + 200 + 4 + 1000 + 4 + 4 + 8 + 8 + 33 + 32 + 4 + 8 + 1, // discriminator + string lengths + data + vecs + bump
        seeds = [b"proposal", &group.key().to_bytes()[..8], &proposal_id.as_bytes()[..8]],
        bump
    )]
    pub proposal: Account<'info, Proposal>,

    #[account(
        mut,
        constraint = group.authority == authority.key() @ DaoError::Unauthorized
    )]
    pub group: Account<'info, Group>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct VoteOnProposal<'info> {
    #[account(mut)]
    pub proposal: Account<'info, Proposal>,

    #[account(mut)]
    pub voter: Signer<'info>,

    /// CHECK: This account is only used for SPL token voting, not for SOL voting
    pub voter_token_account: Option<AccountInfo<'info>>,

    /// CHECK: This account is only used for SPL token voting, not for SOL voting  
    pub token_program: Option<AccountInfo<'info>>,
}

#[derive(Accounts)]
pub struct AddGroupMember<'info> {
    #[account(
        mut,
        constraint = group.authority == authority.key() @ DaoError::Unauthorized
    )]
    pub group: Account<'info, Group>,

    #[account(mut)]
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct RemoveGroupMember<'info> {
    #[account(
        mut,
        constraint = group.authority == authority.key() @ DaoError::Unauthorized
    )]
    pub group: Account<'info, Group>,

    #[account(mut)]
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(telegram_id: i64)]
pub struct CreateUserAccount<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + 8 + 32 + 8 + 1, // discriminator + telegram_id + wallet_pubkey + created_at + bump
        seeds = [b"user_account", telegram_id.to_le_bytes().as_ref()],
        bump
    )]
    pub user_account: Account<'info, UserAccount>,

    /// CHECK: This is the wallet that will be associated with the user account
    pub user_wallet: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(telegram_id: i64)]
pub struct LoginUser<'info> {
    #[account(
        seeds = [b"user_account", telegram_id.to_le_bytes().as_ref()],
        bump = user_account.bump
    )]
    pub user_account: Account<'info, UserAccount>,
}

#[derive(Accounts)]
pub struct GetAllGroups<'info> {
    #[account(
        seeds = [b"dao_registry"],
        bump = dao_registry.bump
    )]
    pub dao_registry: Account<'info, DaoRegistry>,
}

// Events
#[event]
pub struct GroupCreatedEvent {
    pub group_id: String,
    pub authority: Pubkey,
    pub group_pubkey: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct ProposalCreatedEvent {
    pub group_id: String,
    pub proposal_id: String,
    pub creator: Pubkey,
    pub proposal_pubkey: Pubkey,
    pub voting_start: i64,
    pub voting_end: i64,
    pub timestamp: i64,
}

#[event]
pub struct VoteCastEvent {
    pub group_id: String,
    pub proposal_id: String,
    pub voter: Pubkey,
    pub choice: u8,
    pub vote_weight: u64,
    pub timestamp: i64,
}

#[event]
pub struct MemberAddedEvent {
    pub group_id: String,
    pub member: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct MemberRemovedEvent {
    pub group_id: String,
    pub member: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct UserAccountCreatedEvent {
    pub telegram_id: i64,
    pub wallet_pubkey: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct UserLoginEvent {
    pub telegram_id: i64,
    pub wallet_pubkey: Pubkey,
    pub timestamp: i64,
}

// Error Codes
#[error_code]
pub enum DaoError {
    #[msg("Group ID too long (max 50 characters)")]
    GroupIdTooLong,
    #[msg("Name too long (max 100 characters)")]
    NameTooLong,
    #[msg("Description too long (max 500 characters)")]
    DescriptionTooLong,
    #[msg("Proposal ID too long (max 50 characters)")]
    ProposalIdTooLong,
    #[msg("Title too long (max 200 characters)")]
    TitleTooLong,
    #[msg("Invalid choice count (must be between 2 and 10)")]
    InvalidChoiceCount,
    #[msg("Invalid voting period")]
    InvalidVotingPeriod,
    #[msg("Voting start time cannot be in the past")]
    VotingStartInPast,
    #[msg("Voting is not currently active")]
    VotingNotActive,
    #[msg("Invalid choice")]
    InvalidChoice,
    #[msg("User has already voted")]
    AlreadyVoted,
    #[msg("Token account is required for token-weighted voting")]
    TokenAccountRequired,
    #[msg("Invalid token mint")]
    InvalidTokenMint,
    #[msg("No voting power")]
    NoVotingPower,
    #[msg("Unauthorized")]
    Unauthorized,
    #[msg("Member already exists")]
    MemberAlreadyExists,
    #[msg("Member not found")]
    MemberNotFound,
    #[msg("Invalid Telegram ID")]
    InvalidTelegramId,
}
