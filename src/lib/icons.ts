// One stroke family: 24px grid, 1.6 stroke, round joins. Paths only; <Icon> draws them.
export const icons = {
  send: 'M7 17 17 7M9 7h8v8',
  receive: 'M17 7 7 17M15 17H7V9',
  back: 'M15 5l-7 7 7 7',
  copy: 'M9 9h10v10H9zM5 15V5h10',
  lock: 'M6 11h12v9H6zM9 11V8a3 3 0 0 1 6 0v3',
  gear: 'M4 7h9M17 7h3M4 17h3M11 17h9M15 5v4M9 15v4', // settings as two sliders: the old cog read as a brightness sun
  close: 'M6 6l12 12M18 6 6 18',
  check: 'M5 12.5l4.5 4.5L19 7.5',
  plus: 'M12 5v14M5 12h14',
  key: 'M14.5 9.5a4 4 0 1 1-1.2-2.9M13.3 6.6 20 13.3l-2 2M17.5 10.8l-2 2', // export keys: never the send arrow (critique 09.10)
  import: 'M12 4v11M7.5 10.5 12 15l4.5-4.5M5 19h14',
  chevron: 'M7 10l5 5 5-5',
  shield: 'M12 3l7 3v5c0 4.5-3 8-7 10-4-2-7-5.5-7-10V6z',
  folder: 'M3 7h7l2 2h9v10H3z',
  swap: 'M7 4v14M3 14l4 4 4-4M17 20V6M21 10l-4-4-4 4',
  // phone tab bar and send field
  wallet: 'M4 7h15a1 1 0 0 1 1 1v11H5a1 1 0 0 1-1-1zM4 7V6a2 2 0 0 1 2-2h11v3M15.5 13.5h1',
  history: 'M3.5 12a8.5 8.5 0 1 0 2.5-6M3.5 4v4h4M12 8v4.5l3 1.8',
  book: 'M6 4h13v16H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM4 17a2 2 0 0 1 2-2h13M9 8.5h6',
  paste: 'M9 3.5h6v3H9zM9 5H5.5v15.5h13V5H15',
  scan: 'M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5M4 12h16',
  flip:'M4 12a8 8 0 0 1 14-5.3M20 12a8 8 0 0 1-14 5.3M18 3v4h-4M6 21v-4h4',
} as const
export type IconName = keyof typeof icons
