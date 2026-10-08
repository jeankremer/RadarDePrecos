import { mockIPC } from '@tauri-apps/api/mocks';
mockIPC((cmd, args) => { console.log('[mock]', cmd, args); return null; });
