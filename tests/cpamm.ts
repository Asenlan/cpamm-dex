import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Cpamm } from "../target/types/cpamm";
import {
    createMint, createAccount, mintTo, getAccount,
} from "@solana/spl-token";
import { assert } from "chai";

describe("cpamm-dex", () => {
    const provider = anchor.AnchorProvider.env();
    anchor.setProvider(provider);

    const program = anchor.workspace.Cpamm as Program<Cpamm>;
    const FEE_NUM = new anchor.BN(30);
    const FEE_DENOM = new anchor.BN(10000);

    let payer: anchor.web3.Keypair;
    let mintA: anchor.web3.PublicKey;
    let mintB: anchor.web3.PublicKey;
    let payerA: anchor.web3.PublicKey;
    let payerB: anchor.web3.PublicKey;
    let payerLp: anchor.web3.PublicKey;
    let poolPda: anchor.web3.PublicKey;

    const INIT_A = 1_000_000_000n;  // 1000 tokens (6 dec)
    const INIT_B = 10_000_000_000n; // 10 tokens (9 dec)

    before(async () => {
        payer = anchor.web3.Keypair.generate();
        const sig = await provider.connection.requestAirdrop(
            payer.publicKey,
            100 * anchor.web3.LAMPORTS_PER_SOL
        );
        await provider.connection.confirmTransaction(sig);

        // Create mints (sorted by pubkey)
        const mintKeys = [anchor.web3.Keypair.generate(), anchor.web3.Keypair.generate()];
        mintKeys.sort((a, b) =>
            Buffer.compare(a.publicKey.toBuffer(), b.publicKey.toBuffer())
        );
        mintA = mintKeys[0].publicKey;
        mintB = mintKeys[1].publicKey;

        await createMint(provider.connection, payer, payer.publicKey, null, 6,  mintKeys[0]);
        await createMint(provider.connection, payer, payer.publicKey, null, 9, mintKeys[1]);

        // Create payer token accounts
        payerA  = await createAccount(provider.connection, payer, mintA, payer.publicKey);
        payerB  = await createAccount(provider.connection, payer, mintB, payer.publicKey);

        // Mint tokens
        await mintTo(provider.connection, payer, mintA, payerA, payer.publicKey, INIT_A);
        await mintTo(provider.connection, payer, mintB, payerB, payer.publicKey, INIT_B);

        // Derive pool PDA
        [poolPda] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("pool"), mintA.toBuffer(), mintB.toBuffer()],
            program.programId
        );

        // Derive vault PDAs
        const [vaultA] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("vault-a"), poolPda.toBuffer()],
            program.programId
        );
        const [vaultB] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("vault-b"), poolPda.toBuffer()],
            program.programId
        );
        const [lpMint] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("lp-mint"), poolPda.toBuffer()],
            program.programId
        );

        // Create LP token account for payer
        payerLp = await createAccount(provider.connection, payer, lpMint, payer.publicKey);

        // Initialize pool
        await program.methods
            .initializePool(
                new anchor.BN(INIT_A.toString()),
                new anchor.BN(INIT_B.toString()),
                FEE_NUM,
                FEE_DENOM,
            )
            .accounts({
                payer: payer.publicKey,
                tokenAMint: mintA,
                tokenBMint: mintB,
                payerTokenA: payerA,
                payerTokenB: payerB,
                vaultA: vaultA,
                vaultB: vaultB,
                lpMint: lpMint,
                payerLpAccount: payerLp,
            })
            .signers([payer])
            .rpc();
    });

    it("initializes pool with correct reserves", async () => {
        const pool = await program.account.pool.fetch(poolPda);
        assert.equal(pool.reserveA.toString(), INIT_A.toString());
        assert.equal(pool.reserveB.toString(), INIT_B.toString());
        assert.equal(pool.feeNumerator.toString(), "30");
        assert.equal(pool.isActive, true);
    });

    it("performs a swap A → B", async () => {
        const swapAmount = 100_000n; // 0.1 token A
        const swapIn  = await createAccount(provider.connection, payer, mintA, payer.publicKey);
        const swapOut = await createAccount(provider.connection, payer, mintB, payer.publicKey);
        await mintTo(provider.connection, payer, mintA, swapIn, payer.publicKey, swapAmount);

        const poolBefore = await program.account.pool.fetch(poolPda);

        const [vaultA] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("vault-a"), poolPda.toBuffer()], program.programId
        );
        const [vaultB] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("vault-b"), poolPda.toBuffer()], program.programId
        );

        await program.methods
            .swap(new anchor.BN(swapAmount.toString()), new anchor.BN(1), true)
            .accounts({
                user: payer.publicKey,
                pool: poolPda,
                userInputAccount: swapIn,
                userOutputAccount: swapOut,
                inputVault: vaultA,
                outputVault: vaultB,
                inputMint: mintA,
                outputMint: mintB,
            })
            .signers([payer])
            .rpc();

        const poolAfter = await program.account.pool.fetch(poolPda);
        assert.isTrue(poolAfter.reserveA.gt(poolBefore.reserveA), "reserve A should increase");
        assert.isTrue(poolAfter.reserveB.lt(poolBefore.reserveB), "reserve B should decrease");

        const outBalance = await getAccount(provider.connection, swapOut);
        assert.isTrue(outBalance.amount > 0n, "should receive output tokens");
    });

    it("rejects swap with slippage exceeded", async () => {
        const swapAmount = 1_000_000n;
        const swapIn  = await createAccount(provider.connection, payer, mintA, payer.publicKey);
        const swapOut = await createAccount(provider.connection, payer, mintB, payer.publicKey);
        await mintTo(provider.connection, payer, mintA, swapIn, payer.publicKey, swapAmount);

        const [vaultA] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("vault-a"), poolPda.toBuffer()], program.programId
        );
        const [vaultB] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("vault-b"), poolPda.toBuffer()], program.programId
        );

        try {
            await program.methods
                // demand impossibly high output
                .swap(new anchor.BN(swapAmount.toString()), new anchor.BN("100000000000000"), true)
                .accounts({
                    user: payer.publicKey,
                    pool: poolPda,
                    userInputAccount: swapIn,
                    userOutputAccount: swapOut,
                    inputVault: vaultA,
                    outputVault: vaultB,
                    inputMint: mintA,
                    outputMint: mintB,
                })
                .signers([payer])
                .rpc();
            assert.fail("should have thrown slippage error");
        } catch (err: any) {
            assert.include(err.toString(), "SlippageExceeded");
        }
    });

    it("adds liquidity", async () => {
        const addA = 100_000n;
        const addB = 1_000_000_000n;
        const addInA = await createAccount(provider.connection, payer, mintA, payer.publicKey);
        const addInB = await createAccount(provider.connection, payer, mintB, payer.publicKey);
        await mintTo(provider.connection, payer, mintA, addInA, payer.publicKey, addA);
        await mintTo(provider.connection, payer, mintB, addInB, payer.publicKey, addB);

        const poolBefore = await program.account.pool.fetch(poolPda);
        const [vaultA] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("vault-a"), poolPda.toBuffer()], program.programId
        );
        const [vaultB] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("vault-b"), poolPda.toBuffer()], program.programId
        );
        const [lpMint] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("lp-mint"), poolPda.toBuffer()], program.programId
        );

        await program.methods
            .addLiquidity(new anchor.BN(addA.toString()), new anchor.BN(addB.toString()), new anchor.BN(1))
            .accounts({
                provider: payer.publicKey,
                pool: poolPda,
                providerTokenA: addInA,
                providerTokenB: addInB,
                vaultA: vaultA,
                vaultB: vaultB,
                lpMint: lpMint,
                providerLp: payerLp,
            })
            .signers([payer])
            .rpc();

        const poolAfter = await program.account.pool.fetch(poolPda);
        assert.isTrue(poolAfter.totalLpSupply.gt(poolBefore.totalLpSupply), "LP supply increased");
    });

    it("removes liquidity", async () => {
        const poolBefore = await program.account.pool.fetch(poolPda);
        const lpToBurn = new anchor.BN(
            (BigInt(poolBefore.totalLpSupply.toString()) / 2n).toString()
        );

        const [vaultA] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("vault-a"), poolPda.toBuffer()], program.programId
        );
        const [vaultB] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("vault-b"), poolPda.toBuffer()], program.programId
        );
        const [lpMint] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("lp-mint"), poolPda.toBuffer()], program.programId
        );

        await program.methods
            .removeLiquidity(lpToBurn)
            .accounts({
                provider: payer.publicKey,
                pool: poolPda,
                lpMint: lpMint,
                providerLp: payerLp,
                vaultA: vaultA,
                vaultB: vaultB,
                providerTokenA: payerA,
                providerTokenB: payerB,
            })
            .signers([payer])
            .rpc();

        const poolAfter = await program.account.pool.fetch(poolPda);
        assert.isTrue(poolAfter.totalLpSupply.lt(poolBefore.totalLpSupply), "LP supply decreased");
        assert.isTrue(poolAfter.reserveA.lt(poolBefore.reserveA), "reserve A decreased");
        assert.isTrue(poolAfter.reserveB.lt(poolBefore.reserveB), "reserve B decreased");
    });
});
