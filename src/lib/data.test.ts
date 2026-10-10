import { expect, mock, test } from 'bun:test'
mock.module('./i18n.svelte', () => ({ t: (k: string) => k })) // runes need the Svelte compiler; the check needs none of it
const { isAddress } = await import('./data')

test('a Solana address is 32 bytes; Tron and Bitcoin lookalikes are not', () => {
  expect(isAddress('11111111111111111111111111111111')).toBe(true) // system program: all zero bytes
  expect(isAddress('EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v')).toBe(true) // USDC mint
  expect(isAddress('TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t')).toBe(false) // USDT contract on Tron
  expect(isAddress('1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa')).toBe(false) // Bitcoin P2PKH
  expect(isAddress('3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy')).toBe(false) // Bitcoin P2SH
})
