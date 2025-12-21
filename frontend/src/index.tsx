import { render } from 'solid-js/web';
import { Router, Route } from '@solidjs/router';
import App from './App';
import Sessions from './pages/Sessions';
import Session from './pages/Session';
import './index.css';

const root = document.getElementById('root');

if (root) {
  render(
    () => (
      <Router root={App}>
        <Route path="/" component={Sessions} />
        <Route path="/session/:id" component={Session} />
      </Router>
    ),
    root
  );
}
