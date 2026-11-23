import { createRouter, createWebHistory } from 'vue-router'

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', name: 'dashboard', component: () => import('./views/DashboardView.vue') },
    { path: '/servicios', name: 'services', component: () => import('./views/ServicesView.vue') },
    {
      path: '/servicios/:sid',
      name: 'service',
      component: () => import('./views/ServiceDetailView.vue'),
      props: true,
    },
    {
      path: '/servicios/:sid/endpoints/:eid',
      name: 'endpoint',
      component: () => import('./views/EndpointEditorView.vue'),
      props: true,
    },
    { path: '/historial', name: 'history', component: () => import('./views/HistoryView.vue') },
    { path: '/espacios', name: 'workspaces', component: () => import('./views/WorkspacesView.vue') },
    { path: '/ajustes', name: 'settings', component: () => import('./views/SettingsView.vue') },
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
})
