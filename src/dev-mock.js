// Provisório (completo na Tarefa 13): simula o backend no navegador, fora do Tauri.
import { mockIPC } from '@tauri-apps/api/mocks';

const ml = { clientId: '417769415941125', redirectUri: 'https://gocomercio.com.br/oauth/mercadolivre/callback', hasSecret: false, connected: false, nickname: null };

mockIPC((cmd, args) => {
  switch (cmd) {
    case 'ml_status': return ml;
    case 'plugin:app|version': return '0.0.0-dev';
    default: console.log('[mock]', cmd, args); return null;
  }
});
