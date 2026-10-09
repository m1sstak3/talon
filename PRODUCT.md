# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack
Tauri 2 desktop app (Windows first, Android later): Rust core holds keys and signs; the UI is a web front in the system webview (WebView2 = Chromium). UI framework delegated: Svelte 5 + Vite + TypeScript (compiled, small runtime, native FLIP/crossfade transitions); three.js for the glass card ring, GSAP for choreography. Secure window (passphrase, words, send confirmation) is a separate zero-dependency hand-written page.

## Users
First its owner, daily, on a Windows PC and an Android phone, holding and moving their own crypto. Open source: anyone can download and run it.

## Product Purpose
A personal multi-chain wallet: accounts, balances (SOL + SPL tokens), send/receive with QR, history, address book. Solana first, EVM next; a new chain is an added module. Success = the owner trusts it with real money and enjoys opening it.

## Positioning
A wallet that stays yours: no telemetry, keys never leave the machine. Its edge (owner 08.10): an AI agent can work with it (read balances first, more mechanics later), and add-ons let people customise it without coding. Private integrations live in add-ons, never in the app. It is also a showcase piece: the craft of the interface is part of the point.

## Operating Context
- Desktop window on Windows 11, mouse + keyboard; later phone, touch.
- Networks: Solana mainnet / devnet / local test validator.

## Capabilities and Constraints
- Every send shows amount, full fee, network, recipient and needs explicit confirm; active network is checked first.
- Passphrase asked on every send; secrets are entered only in the secure window.
- Nothing secret in logs, URLs, crash reports or screenshots; secure screens excluded from capture.
- Agents never sign mainnet transactions.
- Undecided: fiat prices source (not requested yet), dApp connection (not requested).

## Brand Commitments
Name: Talon (working). UI copy in Russian. Visual direction chosen by owner 2026-10-07: «Колода» (glass card ring), maximum motion and effects.

## Evidence on Hand
No real balances or history in the prototype: demo values are labelled demo data. No logos except official token icons.

## Product Principles
1. Money truth over beauty: an unknown balance reads "unknown", never 0; amounts are exact.
2. The confirm moment is sacred: one place, one exact transaction, nothing can change it after the look.
3. Showcase where it matters (unlock, send, receive), quiet where it repeats (lists, history).
4. A new network is a new module, never a new app.

## Accessibility & Inclusion
Respect reduced motion (full functionality without the 3D ring); keyboard-operable everything; contrast on dark glass checked.
