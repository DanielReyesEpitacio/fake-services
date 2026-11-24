import { createRouter, createWebHistory } from 'vue-router'
import { RouteNames } from './route-names.ts'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/',
      name: RouteNames.dashboard,
      component: () => import('@/app/modules/dashboard/pages/DashboardPage.vue'),
    },
    {
      path: '/servicios',
      name: RouteNames.services,
      component: () => import('@/app/modules/services/pages/IndexServicesPage.vue'),
    },
    {
      path: '/servicios/:sid',
      name: RouteNames.service,
      component: () => import('@/app/modules/services/pages/ServiceDetailPage.vue'),
      props: true,
    },
    {
      path: '/servicios/:sid/endpoints/:eid',
      name: RouteNames.endpoint,
      component: () => import('@/app/modules/endpoints/pages/EndpointEditorPage.vue'),
      props: true,
    },
    {
      path: '/servicios/:sid/workflows/:wid',
      name: RouteNames.workflow,
      component: () => import('@/app/modules/workflows/pages/WorkflowEditorPage.vue'),
      props: true,
    },
    {
      path: '/ejecuciones',
      name: RouteNames.executions,
      component: () => import('@/app/modules/executions/pages/ExecutionsPage.vue'),
    },
    {
      path: '/ejecuciones/:id',
      name: RouteNames.execution,
      component: () => import('@/app/modules/executions/pages/ExecutionDetailPage.vue'),
      props: true,
    },
    {
      path: '/importar-exportar',
      name: RouteNames.transfer,
      component: () => import('@/app/modules/transfer/pages/ImportExportPage.vue'),
    },
    {
      path: '/historial',
      name: RouteNames.history,
      component: () => import('@/app/modules/history/pages/HistoryPage.vue'),
    },
    {
      path: '/espacios',
      name: RouteNames.workspaces,
      component: () => import('@/app/modules/workspaces/pages/WorkspacesPage.vue'),
    },
    {
      path: '/ajustes',
      name: RouteNames.settings,
      component: () => import('@/app/modules/settings/pages/SettingsPage.vue'),
    },
    {
      path: '/apariencia',
      name: RouteNames.appearance,
      component: () => import('@/app/modules/settings/pages/AppearancePage.vue'),
    },
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
})

export default router
export { RouteNames }
