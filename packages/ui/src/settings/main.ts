import { createApp } from 'vue';
import { createPinia } from 'pinia';
import App from './App.vue';
import { bootPreferences } from '../bootstrap';
import { disableBrowserChrome } from '../chrome';
import { i18n } from '../i18n';
import '../panel/style.css';
import './style.css';

disableBrowserChrome();

document.documentElement.classList.add('settings-window');
document.body.classList.add('settings-window');

void bootPreferences();
createApp(App).use(createPinia()).use(i18n).mount('#app');
