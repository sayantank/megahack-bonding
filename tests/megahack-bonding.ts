import * as anchor from "@project-serum/anchor";
import { Program } from "@project-serum/anchor";
import {
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
} from "@solana/web3.js";
import { expect } from "chai";
import { MegahackBonding } from "../target/types/megahack_bonding";

const QUOTE_DECIMALS = 6;
const BOND_DECIMALS = 6;

describe("megahack-bonding", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.MegahackBonding as Program<MegahackBonding>;

  const alice = Keypair.generate();
  console.log(`Alice: ${alice.publicKey.toBase58()}`);

  const bob = Keypair.generate();
  console.log(`Bob: ${bob.publicKey.toBase58()}`);

  let QUOTE_MINT: PublicKey;

  let bobBondTokenAccount: PublicKey;
  let bobQuoteTokenAccount: PublicKey;

  let aliceBondAccount: PublicKey;
  let aliceBondMint: PublicKey;
  let aliceBondVault: PublicKey;

  before(async () => {
    // Airdrop Alice some SOL
    const aliceAirdropSig = await provider.connection.requestAirdrop(
      alice.publicKey,
      1000 * LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(aliceAirdropSig);

    // Airdrop Bob some SOL
    const bobAirdropSig = await provider.connection.requestAirdrop(
      bob.publicKey,
      1000 * LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction(bobAirdropSig);

    // Create a mint to be used as the quote token.
    QUOTE_MINT = await createMint(
      provider.connection,
      alice,
      alice.publicKey,
      alice.publicKey,
      QUOTE_DECIMALS,
      undefined,
      { commitment: "confirmed" }
    );

    // Get the associated token account for Bob which will store his quote tokens.
    const bobQuoteAccount = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      bob,
      QUOTE_MINT,
      bob.publicKey
    );
    bobQuoteTokenAccount = bobQuoteAccount.address;

    // Mint some quote tokens to Bob, so that he can buy bond tokens.
    await mintTo(
      provider.connection,
      bob,
      QUOTE_MINT,
      bobQuoteAccount.address,
      alice,
      BigInt(1000 * Math.pow(10, QUOTE_DECIMALS)),
      undefined,
      { commitment: "confirmed" }
    );
  });

  it("can initialize bond!", async () => {
    // Add your test here.
    [aliceBondAccount] = await PublicKey.findProgramAddressSync(
      [Buffer.from("bond"), alice.publicKey.toBuffer()],
      program.programId
    );
    [aliceBondMint] = await PublicKey.findProgramAddressSync(
      [Buffer.from("mint"), aliceBondAccount.toBuffer()],
      program.programId
    );
    [aliceBondVault] = await PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), aliceBondAccount.toBuffer()],
      program.programId
    );

    const tx = await program.methods
      .initBond(BOND_DECIMALS, "Alice", "ALCE")
      .accounts({
        owner: alice.publicKey,
        bond: aliceBondAccount,
        bondMint: aliceBondMint,
        quoteMint: QUOTE_MINT,
        vault: aliceBondVault,
        rent: SYSVAR_RENT_PUBKEY,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([alice])
      .rpc();

    console.log("initBond tx signature: ", tx);

    const aliceBond = await program.account.bond.fetch(aliceBondAccount);

    expect(aliceBond.owner.toBase58()).to.equal(alice.publicKey.toBase58());
    expect(aliceBond.bondMint.toBase58()).to.equal(aliceBondMint.toBase58());
    expect(aliceBond.quoteMint.toBase58()).to.equal(QUOTE_MINT.toBase58());
    expect(aliceBond.vault.toBase58()).to.equal(aliceBondVault.toBase58());
    expect(aliceBond.quoteMintDecimals).to.equal(QUOTE_DECIMALS);
    expect(aliceBond.bondMintDecimals).to.equal(BOND_DECIMALS);
    expect(aliceBond.name).to.equal("Alice");
    expect(aliceBond.symbol).to.equal("ALCE");
  });

  it("can buy bond tokens!", async () => {
    ({ address: bobBondTokenAccount } = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      bob,
      aliceBondMint,
      bob.publicKey,
      false
    ));

    await program.methods
      .mintBond(new anchor.BN(30 * Math.pow(10, BOND_DECIMALS)))
      .accounts({
        buyer: bob.publicKey,
        bondOwner: alice.publicKey,
        bond: aliceBondAccount,
        bondMint: aliceBondMint,
        quoteMint: QUOTE_MINT,
        vault: aliceBondVault,
        buyerQuoteTokenAccount: bobQuoteTokenAccount,
        buyerBondTokenAccount: bobBondTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([bob])
      .rpc({ commitment: "confirmed" });

    const balance = await provider.connection.getTokenAccountBalance(
      bobBondTokenAccount,
      "confirmed"
    );

    expect(balance.value.uiAmount).to.equal(30);
  });

  it("can sell bond tokens!", async () => {
    await program.methods
      .burnBond(new anchor.BN(10 * Math.pow(10, BOND_DECIMALS)))
      .accounts({
        seller: bob.publicKey,
        bondOwner: alice.publicKey,
        bond: aliceBondAccount,
        bondMint: aliceBondMint,
        quoteMint: QUOTE_MINT,
        vault: aliceBondVault,
        sellerQuoteTokenAccount: bobQuoteTokenAccount,
        sellerBondTokenAccount: bobBondTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([bob])
      .rpc({ commitment: "confirmed" });

    const balance = await provider.connection.getTokenAccountBalance(
      bobBondTokenAccount,
      "confirmed"
    );

    expect(balance.value.uiAmount).to.equal(20);
  });
});
