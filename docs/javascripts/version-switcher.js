// Version switcher for Polarway documentation
// Adds a version dropdown to the header

(function() {
  'use strict';

  // Version data
  const versions = [
    { version: 'v0.54.0 (dev)', url: '/en/v0.54.0/', label: 'Latest (Development)' },
    { version: 'v0.53.0', url: '/en/v0.53.0/', label: 'Stable' },
    { version: 'v0.52.0', url: '/en/v0.52.0/', label: 'Previous' },
  ];

  function createVersionDropdown() {
    const header = document.querySelector('.md-header__inner');
    if (!header) return;

    const nav = header.querySelector('.md-header__topic');
    if (!nav) return;

    // Create dropdown container
    const dropdown = document.createElement('div');
    dropdown.className = 'md-header__version';
    dropdown.style.cssText = 'position: relative; margin-left: 1rem;';

    // Current version detection
    const currentPath = window.location.pathname;
    let currentVersion = versions.find(v => currentPath.includes(v.url.replace('/en/', '/'))) || versions[1];

    dropdown.innerHTML = `
      <button class="md-header__version-btn md-button" 
              aria-label="Switch version" 
              aria-haspopup="listbox"
              aria-expanded="false"
              style="display: flex; align-items: center; gap: 0.5rem; padding: 0.375rem 0.75rem; border-radius: 0.375rem; background: var(--md-code-bg-color); border: 1px solid var(--md-default-fg-color--lightest); font-size: 0.75rem; font-weight: 600; color: var(--md-primary-fg-color);">
        <span>${currentVersion.version}</span>
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="flex-shrink: 0;">
          <polyline points="6 9 12 15 18 9"></polyline>
        </svg>
      </button>
      <div class="md-header__version-menu" role="listbox" style="position: absolute; top: 100%; right: 0; margin-top: 0.5rem; min-width: 180px; background: var(--md-header-bg-color); border: 1px solid var(--md-default-fg-color--lightest); border-radius: 0.5rem; box-shadow: 0 10px 40px rgba(0,0,0,0.15); opacity: 0; visibility: hidden; transform: translateY(-8px); transition: all 0.2s ease; z-index: 100; overflow: hidden;">
        ${versions.map(v => `
          <a href="${v.url}" role="option" class="md-header__version-item" style="display: block; padding: 0.625rem 1rem; color: var(--md-default-fg-color); text-decoration: none; transition: background 0.15s; border-bottom: 1px solid var(--md-default-fg-color--lightest);" ${v.version === currentVersion.version ? 'aria-selected="true" style="background: var(--md-primary-fg-color); color: white;"' : ''}>
            <div style="font-weight: 600; font-size: 0.8125rem;">${v.version}</div>
            <div style="font-size: 0.6875rem; opacity: 0.7; margin-top: 0.125rem;">${v.label}</div>
          </a>
        `).join('')}
      </div>
    `;

    const btn = dropdown.querySelector('.md-header__version-btn');
    const menu = dropdown.querySelector('.md-header__version-menu');
    let isOpen = false;

    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      isOpen = !isOpen;
      btn.setAttribute('aria-expanded', isOpen);
      if (isOpen) {
        menu.style.opacity = '1';
        menu.style.visibility = 'visible';
        menu.style.transform = 'translateY(0)';
      } else {
        menu.style.opacity = '0';
        menu.style.visibility = 'hidden';
        menu.style.transform = 'translateY(-8px)';
      }
    });

    document.addEventListener('click', (e) => {
      if (!dropdown.contains(e.target)) {
        isOpen = false;
        btn.setAttribute('aria-expanded', 'false');
        menu.style.opacity = '0';
        menu.style.visibility = 'hidden';
        menu.style.transform = 'translateY(-8px)';
      }
    });

    // Close on escape
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && isOpen) {
        isOpen = false;
        btn.setAttribute('aria-expanded', 'false');
        menu.style.opacity = '0';
        menu.style.visibility = 'hidden';
        menu.style.transform = 'translateY(-8px)';
        btn.focus();
      }
    });

    nav.appendChild(dropdown);
  }

  // Initialize when DOM is ready
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', createVersionDropdown);
  } else {
    createVersionDropdown();
  }
})();