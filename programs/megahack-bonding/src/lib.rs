use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

fn get_area_under_curve(
    left_supply: u64,
    right_supply: u64,
    bond_decimals: u8,
    quote_decimals: u8,
) -> u64 {
    let num: u128 = u128::from(right_supply - left_supply)
        .checked_mul(
            u128::from(right_supply)
                .pow(2)
                .checked_add(
                    u128::from(left_supply)
                        .pow(2)
                        .checked_add(
                            u128::from(right_supply)
                                .checked_mul(u128::from(left_supply))
                                .unwrap(),
                        )
                        .unwrap(),
                )
                .unwrap(),
        )
        .unwrap();

    let den = 10_u128.pow((u32::from(bond_decimals * 3)) - u32::from(quote_decimals) + 3);

    let total_price: u64 = num.checked_div(den).unwrap().try_into().unwrap();

    total_price
}

#[program]
pub mod megahack_bonding {
    use anchor_spl::token::{self, Burn, MintTo, Transfer};

    use super::*;

    pub fn init_bond(
        ctx: Context<InitBond>,
        _mint_decimals: u8,
        name: String,
        symbol: String,
    ) -> Result<()> {
        // Input validation
        if name.len() > 10 {
            return err!(BondError::NameTooLong);
        }
        if symbol.len() > 4 {
            return err!(BondError::SymbolTooLong);
        }

        // Populating Bond account data
        let bond = &mut ctx.accounts.bond;
        bond.owner = ctx.accounts.owner.key();
        bond.bond_mint = ctx.accounts.bond_mint.key();
        bond.quote_mint = ctx.accounts.quote_mint.key();
        bond.bond_mint_decimals = ctx.accounts.bond_mint.decimals;
        bond.quote_mint_decimals = ctx.accounts.quote_mint.decimals;
        bond.vault = ctx.accounts.vault.key();
        bond.name = name;
        bond.symbol = symbol;
        bond.bump = *ctx.bumps.get("bond").unwrap();

        Ok(())
    }

    // amount provided is in atomic value, or in terms of the smallest quantfiable amount of the bond token.;lk
    pub fn mint_bond(ctx: Context<MintBond>, amount: u64) -> Result<()> {
        // Current circulating supply of the Bond token.
        let current_supply = ctx.accounts.bond_mint.supply;

        // Calcualting price to buy the specified amount of Bond tokens.
        let total_price = get_area_under_curve(
            current_supply,
            current_supply.checked_add(amount).unwrap(),
            ctx.accounts.bond.bond_mint_decimals,
            ctx.accounts.bond.quote_mint_decimals,
        );

        // Transferring the total_price amount of quote_tokens to the vault.
        let transfer_cpi_accounts = Transfer {
            from: ctx.accounts.buyer_quote_token_account.to_account_info(),
            to: ctx.accounts.vault.to_account_info(),
            authority: ctx.accounts.buyer.to_account_info(),
        };
        let transfer_cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_cpi_accounts,
        );
        token::transfer(transfer_cpi_ctx, total_price)?;
        msg!("Transferred {} quote_tokens to vault.", total_price);

        // Minting the specified amount of Bond tokens.
        let mint_cpi_accounts = MintTo {
            mint: ctx.accounts.bond_mint.to_account_info(),
            to: ctx.accounts.buyer_bond_token_account.to_account_info(),
            authority: ctx.accounts.bond.to_account_info(),
        };
        // The MintTo instruction requires the mint authority to sign the transaction.
        // The mint authority is the Bond account, which is a PDA.
        // Hence, to sign for a PDA derived the program, we will require the seeds.
        let seeds = &[
            b"bond".as_ref(),
            ctx.accounts.bond.owner.as_ref(),
            &[ctx.accounts.bond.bump],
        ];
        let signers = &[&seeds[..]];
        let mint_cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            mint_cpi_accounts,
            signers,
        );
        token::mint_to(mint_cpi_ctx, amount)?;
        msg!("Minted {} Bond tokens.", amount);

        Ok(())
    }

    pub fn burn_bond(ctx: Context<BurnBond>, amount: u64) -> Result<()> {
        let current_supply = ctx.accounts.bond_mint.supply;

        // Calculating the amount of quote_tokens to be returned for burning.
        let total_return = get_area_under_curve(
            current_supply.checked_sub(amount).unwrap(),
            current_supply,
            ctx.accounts.bond.bond_mint_decimals,
            ctx.accounts.bond.quote_mint_decimals,
        );

        // Burning the specified amount of Bond tokens.
        // Doesn't matter if you burn first, or transfer first. Everything occurs atomically.
        let burn_cpi_accounts = Burn {
            mint: ctx.accounts.bond_mint.to_account_info(),
            from: ctx.accounts.seller_bond_token_account.to_account_info(),
            authority: ctx.accounts.seller.to_account_info(),
        };
        let burn_cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            burn_cpi_accounts,
        );
        token::burn(burn_cpi_ctx, amount)?;
        msg!("Burnt {} Bond tokens.", amount);

        // Transferring the total_return amount of quote_tokens to the seller.
        let transfer_cpi_accounts = Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.seller_quote_token_account.to_account_info(),
            authority: ctx.accounts.bond.to_account_info(),
        };
        let seeds = &[
            b"bond".as_ref(),
            ctx.accounts.bond.owner.as_ref(),
            &[ctx.accounts.bond.bump],
        ];
        let signers = &[&seeds[..]];
        let transfer_cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer_cpi_accounts,
            signers,
        );
        token::transfer(transfer_cpi_ctx, total_return)?;
        msg!("Transferred {} quote_tokens to seller.", total_return);

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(mint_decimals: u8)]
pub struct InitBond<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        init,
        seeds = [b"bond".as_ref(), owner.key().as_ref()],
        bump,
        payer = owner,
        space = Bond::LEN,
    )]
    pub bond: Account<'info, Bond>,

    /// Authority of Mint is the Bond account, which is a PDA derived from the Bond account.
    /// Hence, the program, can sign any transaction involving minting this token.
    /// Similarly, the program will be be able to freeze the minting of this token.
    #[account(
        init,
        seeds = [b"mint".as_ref(), bond.key().as_ref()],
        bump,
        payer = owner,
        mint::decimals = mint_decimals,
        mint::authority = bond,
        mint::freeze_authority = bond,
    )]
    pub bond_mint: Account<'info, Mint>,

    pub quote_mint: Account<'info, Mint>,

    /// The token account is created as a PDA derived from the Bond account
    /// This account will act as the vault where the quote tokens are locked.
    /// The authority of the token account is the Bond account.
    /// Hence, the owner can't transfer the quote tokens from the token account, but this program can.
    #[account(
        init,
        seeds = [b"vault".as_ref(), bond.key().as_ref()],
        bump,
        payer = owner,
        token::mint = quote_mint,
        token::authority = bond,
    )]
    pub vault: Account<'info, TokenAccount>,

    pub rent: Sysvar<'info, Rent>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintBond<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,

    /// CHECK: This is owner of the Bond account whose tokens you want to mint.
    pub bond_owner: AccountInfo<'info>,

    /// Validating that the Bond account provided is infact for the bond_owner provided.
    #[account(
        seeds = [b"bond".as_ref(), bond_owner.key().as_ref()],
        bump,
    )]
    pub bond: Account<'info, Bond>,

    /// Validating that the bond_mint is the bond_mint stored in the Bond account.
    /// Needs to be mutable since we will be minting some of this token.
    #[account(mut, address = bond.bond_mint)]
    pub bond_mint: Account<'info, Mint>,

    /// Validating that the quote_mint is the quote_mint stored in the Bond account.
    #[account(address = bond.quote_mint)]
    pub quote_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [b"vault".as_ref(), bond.key().as_ref()],
        bump,
        token::mint = quote_mint,
        token::authority = bond,
    )]
    pub vault: Account<'info, TokenAccount>,

    /// Associated Token Account is a special type of Token Account
    /// Every account can have multiple token accounts, but one associated token account for a given mint.
    /// It is also infact a PDA, but derived from the Associated Token Program.
    #[account(
        mut,
        associated_token::mint = bond_mint,
        associated_token::authority = buyer,
    )]
    pub buyer_bond_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = quote_mint,
        associated_token::authority = buyer,
    )]
    pub buyer_quote_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct BurnBond<'info> {
    #[account(mut)]
    pub seller: Signer<'info>,

    /// CHECK: This is owner of the Bond account whose tokens you want to burn.
    pub bond_owner: AccountInfo<'info>,

    #[account(
        seeds = [b"bond".as_ref(), bond_owner.key().as_ref()],
        bump,
    )]
    pub bond: Account<'info, Bond>,

    #[account(mut, address = bond.bond_mint)]
    pub bond_mint: Account<'info, Mint>,

    #[account(mut, address = bond.quote_mint)]
    pub quote_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [b"vault".as_ref(), bond.key().as_ref()],
        bump,
        token::mint = quote_mint,
        token::authority = bond,
    )]
    pub vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = bond_mint,
        associated_token::authority = seller,
    )]
    pub seller_bond_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = quote_mint,
        associated_token::authority = seller,
    )]
    pub seller_quote_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[account]
pub struct Bond {
    pub owner: Pubkey,
    pub bond_mint: Pubkey,
    pub bond_mint_decimals: u8,
    pub quote_mint: Pubkey,
    pub quote_mint_decimals: u8,
    pub vault: Pubkey,
    pub bump: u8,
    /// Name can have a maximum of 10 chars
    pub name: String,
    /// Symbol can have a maximum of 4 chars
    pub symbol: String,
}

impl Bond {
    // Discriminator takes 8 bytes
    pub const LEN: usize = 8 + 32 + (32 + 1) + (32 + 1) + 32 + 1 + (4 + (4 * 10)) + (4 + (4 * 4));
}

#[error_code]
pub enum BondError {
    #[msg("The name provided can not be longer than 10 characters.")]
    NameTooLong,
    #[msg("The symbol provided can not be longer than 4 characters.")]
    SymbolTooLong,
}
