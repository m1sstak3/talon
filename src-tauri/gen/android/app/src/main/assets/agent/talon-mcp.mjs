#!/usr/bin/env node
// Talon MCP server (stdio) for AI agents. Zero dependencies: Node 18+.
// It relays four calls to the running wallet over a local Windows named pipe. Nothing here can sign or export:
// a transfer is only a REQUEST, which the owner confirms in the wallet window with the password.
// Setup: Settings -> «Доступ агента» on in the wallet; then register this file as an MCP server, e.g.
//   claude mcp add talon -- node "<install folder>\agent\talon-mcp.mjs"
import { readFileSync } from 'node:fs'
import { connect } from 'node:net'
import { join } from 'node:path'

// the wallet writes the pipe name and a per-launch secret next to its vault (owner-only folder)
const discovery = process.env.CW_AGENT_FILE || join(process.env.APPDATA || '', 'io.github.m1sstak3.talon', 'agent.json')

function wallet(method, params = {}) {
  let d
  try { d = JSON.parse(readFileSync(discovery, 'utf8')) } catch { throw new Error('Talon is not running, or agent access is off (Settings -> «Доступ агента»)') }
  return new Promise((resolve, reject) => {
    const s = connect(d.pipe)
    let buf = ''
    const t = setTimeout(() => { s.destroy(); reject(new Error('the wallet did not answer in 120 s')) }, 120_000)
    s.on('error', () => { clearTimeout(t); reject(new Error('Talon is not running, or agent access is off')) })
    s.on('data', c => {
      buf += c
      const i = buf.indexOf('\n')
      if (i < 0) return
      clearTimeout(t); s.end()
      const r = JSON.parse(buf.slice(0, i))
      r.error ? reject(new Error(r.error)) : resolve(r.result)
    })
    s.write(JSON.stringify({ id: 1, secret: d.secret, method, params }) + '\n')
  })
}

const network = { type: 'string', enum: ['mainnet', 'devnet', 'local'], description: 'Solana cluster; default mainnet' }
const TOOLS = [
  { name: 'wallet_accounts', description: 'List the wallet\'s own accounts: id, name, Solana address, EVM address. Public data only.', inputSchema: { type: 'object', properties: {} } },
  { name: 'wallet_balances', description: 'SOL and token balances of the own accounts on a Solana network (all accounts, or one by id or address). Amounts are integers in base units (lamports = 1e-9 SOL).',
    inputSchema: { type: 'object', properties: { network, account: { type: 'string', description: 'account id or Solana address; omit for all' } } } },
  { name: 'wallet_request_transfer', description: 'Ask the owner to send SOL from up to 50 OWN accounts to ONE destination. Creates a plan the owner must confirm in the wallet window with the password; nothing is sent until then. amount: decimal SOL ("0.25") or "max" (whole balance minus the network fee). One pending request at a time, at most 5 requests per minute. Returns a request id.',
    inputSchema: { type: 'object', required: ['to', 'sources'], properties: { network, to: { type: 'string', description: 'destination Solana address' },
      sources: { type: 'array', maxItems: 50, items: { type: 'object', required: ['account', 'amount'], properties: { account: { type: 'string', description: 'own account id or its Solana address' }, amount: { type: 'string' } } } } } } },
  { name: 'wallet_request_status', description: 'State of a transfer request: waiting (for the owner), closed (rejected/expired, with reason), or confirmed with each transfer\'s signature and network state (pending/confirmed/finalized/failed/not-sent).',
    inputSchema: { type: 'object', required: ['id'], properties: { id: { type: 'string' } } } },
]
const METHOD = { wallet_accounts: 'accounts', wallet_balances: 'balances', wallet_request_transfer: 'request_transfer', wallet_request_status: 'request_status' }

const send = m => process.stdout.write(JSON.stringify(m) + '\n')
async function handle(m) {
  const { id, method, params } = m
  if (id === undefined) return // notifications (initialized, cancelled)
  if (method === 'initialize') return send({ jsonrpc: '2.0', id, result: { protocolVersion: params?.protocolVersion || '2025-06-18', capabilities: { tools: {} }, serverInfo: { name: 'talon', version: '0.2.0' } } })
  if (method === 'ping') return send({ jsonrpc: '2.0', id, result: {} })
  if (method === 'tools/list') return send({ jsonrpc: '2.0', id, result: { tools: TOOLS } })
  if (method === 'tools/call') {
    const call = METHOD[params?.name]
    if (!call) return send({ jsonrpc: '2.0', id, error: { code: -32602, message: `unknown tool ${params?.name}` } })
    try { send({ jsonrpc: '2.0', id, result: { content: [{ type: 'text', text: JSON.stringify(await wallet(call, params.arguments || {}), null, 1) }] } }) }
    catch (e) { send({ jsonrpc: '2.0', id, result: { isError: true, content: [{ type: 'text', text: String(e.message || e) }] } }) }
    return
  }
  send({ jsonrpc: '2.0', id, error: { code: -32601, message: `method not found: ${method}` } })
}

let inbuf = ''
process.stdin.setEncoding('utf8')
process.stdin.on('data', c => {
  inbuf += c
  for (let i; (i = inbuf.indexOf('\n')) >= 0;) {
    const line = inbuf.slice(0, i).trim(); inbuf = inbuf.slice(i + 1)
    if (line) { try { handle(JSON.parse(line)) } catch { send({ jsonrpc: '2.0', id: null, error: { code: -32700, message: 'parse error' } }) } }
  }
})
