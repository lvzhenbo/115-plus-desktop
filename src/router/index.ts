import { createRouter, createWebHistory } from 'vue-router';

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/',
      name: 'Home',
      component: () => import('@/views/Home/HomeView.vue'),
    },
    {
      path: '/login',
      name: 'Login',
      component: () => import('@/views/Login/LoginView.vue'),
    },
  ],
});

export default router;
