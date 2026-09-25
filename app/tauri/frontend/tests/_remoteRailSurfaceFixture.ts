/**
 * Minimal DOM fixture the remote-history rail keyboard tests
 * use to exercise the focus chain without standing up a full
 * Svelte component. The fixture uses the shared
 * `_domPolyfill.installDomPolyfill` helper so the tests get
 * the `document`, `HTMLElement`, `HTMLInputElement` and
 * friends globals the helper expects; the fixture also
 * installs the parent-chain metadata (`dataset.testid`) the
 * guard walks up to find the rail root.
 */

import { installDomPolyfill, type DomElement } from "./_domPolyfill.ts";

export interface RailSurfaceFixture {
  railSurface: DomElement;
  menuButton: DomElement;
  importButton: DomElement;
  sidebar: DomElement;
  sidebarButton: DomElement;
  sidebarSearch: DomElement;
  siblingPanelButton: DomElement;
  siblingLink: DomElement;
  dispose: () => void;
}

function createRemoteRailSurface(): RailSurfaceFixture {
  const { document, restore } = installDomPolyfill();
  const root = document.createElement("main");
  const sidebar = document.createElement("aside");
  sidebar.setAttribute("data-testid", "app-sidebar");
  const sidebarSearch = document.createElement("input");
  sidebarSearch.setAttribute("type", "search");
  sidebarSearch.setAttribute("data-testid", "app-sidebar-search");
  const sidebarButton = document.createElement("button");
  sidebarButton.setAttribute("data-testid", "app-sidebar-toggle");
  sidebar.appendChild(sidebarSearch);
  sidebar.appendChild(sidebarButton);

  const railSurface = document.createElement("section");
  railSurface.setAttribute("data-testid", "remote-history-rail");
  const card = document.createElement("article");
  card.setAttribute("data-testid", "remote-preview-card");
  const menuButton = document.createElement("button");
  menuButton.setAttribute("data-testid", "remote-preview-card-menu-button");
  const importButton = document.createElement("button");
  importButton.setAttribute("data-testid", "remote-preview-card-import");
  card.appendChild(menuButton);
  card.appendChild(importButton);
  railSurface.appendChild(card);

  const siblingPanel = document.createElement("section");
  siblingPanel.setAttribute("data-testid", "app-local-rail");
  const siblingPanelButton = document.createElement("button");
  siblingPanelButton.setAttribute("data-testid", "app-local-rail-pager-next");
  siblingPanel.appendChild(siblingPanelButton);
  const siblingLink = document.createElement("a");
  siblingLink.setAttribute("href", "#");
  siblingLink.setAttribute("data-testid", "app-external-link");
  siblingPanel.appendChild(siblingLink);

  root.appendChild(sidebar);
  root.appendChild(railSurface);
  root.appendChild(siblingPanel);
  document.documentElement.appendChild(root);

  return {
    railSurface,
    menuButton,
    importButton,
    sidebar,
    sidebarButton,
    sidebarSearch,
    siblingPanelButton,
    siblingLink,
    dispose: restore,
  };
}

export { createRemoteRailSurface };