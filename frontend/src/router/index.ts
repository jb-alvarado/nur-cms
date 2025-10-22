import { createRouter, createWebHistory } from 'vue-router'
import HomeView from '../views/HomeView.vue'

import { useAuth } from '@/stores/auth'

const router = createRouter({
    history: createWebHistory(import.meta.env.BASE_URL),
    routes: [
        {
            path: '/',
            name: 'home',
            component: HomeView,
        },
        {
            path: '/user',
            name: 'user',
            component: () => import('../views/UserView.vue'),
        },
    ],
})

router.beforeEach(async (to, from, next) => {
    const auth = useAuth()

    if (from.name) {
        await auth.inspectToken()
    }

    if (!auth.isLogin && !String(to.name).includes('home')) {
        next('/')
    } else {
        next()
    }
})

export default router
