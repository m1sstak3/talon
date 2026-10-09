// Network card colours (owner 07.10): the light graphite of the first cards balanced with Codex's «smoked alloys» —
// each network keeps a recognisable hue in a lighter metal (45% toward graphite #5b5d63); brand colour on the mark only.
// face / edge are painted sRGB; ACES and the studio light darken them a little on screen.
import type { Card } from './data'

export interface Look {
  face: string // face base (anodized alloy)
  edge: string // rounded edge / body
  mark: string | [string, string] // network mark: brand colour (Solana: its gradient)
  test?: boolean // test network: grey mark + label
}

const L = (face: string, edge: string, mark: Look['mark'], test = false): Look => ({ face, edge, mark, test })
const GREY = '#9a9ca1'

export const NET_LOOK: Record<string, Look> = {
  solana: L('#484654', '#2a2832', ['#9945ff', '#14f195']),
  ethereum: L('#4d5059', '#2c2f37', '#8c9ef0'), // brand #627EEA, lifted to read on dark metal
  base: L('#404b5d', '#232c39', '#4d6bff'), // brand #0000FF, lifted
  arbitrum: L('#435056', '#24313b', '#28a0f0'),
  optimism: L('#55464b', '#32282c', '#ff0420'),
  robinhood: L('#4d5348', '#2c3129', '#ccff00'),
  polygon: L('#514859', '#302937', '#a07cf0'), // brand #8247E5, lifted
  bnb: L('#504c40', '#2f2c24', '#f0b90b'),
  avalanche: L('#554849', '#33292a', '#f05a5b'), // brand #E84142, lifted
  linea: L('#4a4d52', '#2b2e33', '#79e2ff'),
  unichain: L('#54484f', '#33292f', '#f45bc4'), // brand #F50DB4, lifted
  sonic: L('#4b4f58', '#2b2f37', '#ff9c5b'),
  gnosis: L('#474f4c', '#29302d', '#5fb38d'),
  scroll: L('#55504a', '#332f2b', '#f2c38a'),
  ink: L('#4c4858', '#2c2935', '#a48cff'),
  worldchain: L('#4d4d4f', '#2d2d2f', '#e6e6e6'),
  berachain: L('#54503f', '#322f24', '#e8a96a'),
  monad: L('#4e4a5a', '#2e2b36', '#9d8cfb'), // brand #836EF9, lifted
  hyperevm: L('#45504e', '#27302e', '#97fce4'),
  'base-sepolia': L('#525459', '#323337', GREY, true),
  'solana-devnet': L('#4f5056', '#2f3035', GREY, true),
  local: L('#484a4f', '#2b2d31', GREY, true),
  sepolia: L('#525459', '#323337', GREY, true),
}
const NEUTRAL = L('#4d4e4f', '#2e2e2e', '#cfccc4') // custom networks, token cards: no invented brand colour

export const lookFor = (c: Card): Look => NET_LOOK[c.id] ?? NEUTRAL
