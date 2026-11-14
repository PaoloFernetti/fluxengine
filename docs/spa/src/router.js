import { createRouter, createWebHashHistory } from 'vue-router'
import HomePage from './components/HomePage.vue'
import DocPage from './components/DocPage.vue'

export const router = createRouter({
  // Hash-история: собранный docs.html работает из file:// без сервера.
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'home', component: HomePage },
    { path: '/doc/:id', name: 'doc', component: DocPage },
    { path: '/:pathMatch(.*)*', redirect: '/' },
  ],
  scrollBehavior(to, from, savedPosition) {
    if (savedPosition) return savedPosition
    if (to.query.h) return false
    return { top: 0 }
  },
})
