import { useUserStore } from '@/store/user';
import { createRouter, createWebHistory } from 'vue-router';

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    {
      path: '/',
      name: 'Root',
      component: () => import('@/layout/index.vue'),
      redirect: '/home',
      children: [
        {
          path: 'home',
          name: 'Home',
          component: () => import('@/views/Home/HomeView.vue'),
        },
        {
          path: 'recycleBin',
          name: 'RecycleBin',
          component: () => import('@/views/RecycleBin/RecycleBin.vue'),
        },
      ],
    },
    {
      path: '/login',
      name: 'Login',
      component: () => import('@/views/Login/LoginView.vue'),
    },
  ],
});

router.beforeEach((to, _from, next) => {
  const userStore = useUserStore();
  if (to.name !== 'Login' && !userStore.accessToken) {
    next({ name: 'Login' });
  } else {
    next();
  }
});

export default router;
