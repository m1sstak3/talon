import { expect, mock, test } from 'bun:test'
mock.module('./i18n.svelte', () => ({ t: (k: string) => k })) // runes need the Svelte compiler; the check needs none of it
const { isAddress, familyOf, parseScan } = await import('./data')

test('a Solana address is 32 bytes; Tron and Bitcoin lookalikes are not', () => {
  expect(isAddress('11111111111111111111111111111111')).toBe(true) // system program: all zero bytes
  expect(isAddress('EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v')).toBe(true) // USDC mint
  expect(isAddress('TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t')).toBe(false) // USDT contract on Tron
  expect(isAddress('1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa')).toBe(false) // Bitcoin P2PKH
  expect(isAddress('3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy')).toBe(false) // Bitcoin P2SH
})

test('an address names its family', () => {
  expect(familyOf('EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v')).toBe('solana')
  expect(familyOf('0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913')).toBe('evm')
  expect(familyOf('TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t')).toBe('tron')
  expect(familyOf('bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq')).toBe('bitcoin')
  expect(familyOf('1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa')).toBe('bitcoin')
  expect(familyOf('UQBvW8Z5huBkMJYdnfAEM5JqTNkuWX3diqYENkWsIL0XggGG')).toBe('ton')
  expect(familyOf('hello')).toBe('')
})

test('a payment code keeps its network', () => {
  expect(parseScan('ethereum:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913@8453')).toEqual({ to: '0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913', chainId: 8453 })
  expect(parseScan('ethereum:0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913')).toEqual({ to: '0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913', chainId: undefined })
  expect(parseScan('solana:EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v?amount=1.5')).toEqual({ to: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v', amount: '1.5' })
})
