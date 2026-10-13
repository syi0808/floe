import { mountSquircles } from './squircle.js';

const tabs = [...document.querySelectorAll('[role="tab"]')];
const tablist = document.querySelector('[role="tablist"]');
const narrowLayout = window.matchMedia('(max-width: 800px)');
const updateOrientation = () => tablist.setAttribute('aria-orientation', narrowLayout.matches ? 'horizontal' : 'vertical');
updateOrientation();
narrowLayout.addEventListener('change', updateOrientation);

function selectTab(selected) {
  for (const tab of tabs) {
    const active = tab === selected;
    tab.setAttribute('aria-selected', String(active));
    tab.tabIndex = active ? 0 : -1;
    document.getElementById(tab.getAttribute('aria-controls')).hidden = !active;
  }
}

for (const [index, tab] of tabs.entries()) {
  tab.addEventListener('click', () => selectTab(tab));
  tab.addEventListener('keydown', event => {
    const forward = narrowLayout.matches ? 'ArrowRight' : 'ArrowDown';
    const backward = narrowLayout.matches ? 'ArrowLeft' : 'ArrowUp';
    let next;
    if (event.key === forward) next = (index + 1) % tabs.length;
    if (event.key === backward) next = (index - 1 + tabs.length) % tabs.length;
    if (event.key === 'Home') next = 0;
    if (event.key === 'End') next = tabs.length - 1;
    if (next === undefined) return;
    event.preventDefault();
    selectTab(tabs[next]);
    tabs[next].focus();
  });
}

for (const task of document.querySelectorAll('[data-task]')) {
  task.addEventListener('change', () => {
    for (const match of document.querySelectorAll(`[data-task="${task.dataset.task}"]`)) match.checked = task.checked;
    const label = task.nextElementSibling.querySelector('strong')?.textContent || task.nextElementSibling.textContent;
    document.getElementById('demo-status').textContent = `${label}: ${task.checked ? 'completed' : 'not completed'} in the illustration.`;
  });
}

// Enhancement only: content, navigation, and the sample day render without JS.
if ('ResizeObserver' in window) mountSquircles();
