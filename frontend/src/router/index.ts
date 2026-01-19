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
        // {
        //     path: '/author',
        //     name: 'author lists',
        //     component: () => import('../views/author/IndexView.vue'),
        // },
        // {
        //     path: '/article/:id',
        //     name: 'article edit',
        //     component: () => import('../views/article/ArticleEdit.vue'),
        // },
        {
            path: '/verification',
            name: 'verification',
            component: () => import('../views/VerificationView.vue'),
        },
        {
            path: '/configuration',
            name: 'configuration',
            component: () => import('../views/ConfigurationView.vue'),
        },
        {
            path: '/:type',
            name: 'content type',
            component: () => import('../views/content/IndexView.vue'),
        },
        {
            path: '/:type/:id',
            name: 'content edit',
            component: () => import('../views/content/EditView.vue'),
        },
        {
            path: '/:type/:id/:group_id',
            name: 'group content edit',
            component: () => import('../views/content/EditView.vue'),
        },
        // {
        //     path: '/page',
        //     name: 'page',
        //     component: () => import('../views/page/PageView.vue'),
        // },
        // {
        //     path: '/page/:id',
        //     name: 'page edit',
        //     component: () => import('../views/page/PageEdit.vue'),
        // },
        // {
        //     path: '/event',
        //     name: 'event',
        //     component: () => import('../views/event/EventView.vue'),
        // },
        {
            path: '/media',
            name: 'media',
            component: () => import('../views/MediaView.vue'),
        },
        {
            path: '/user',
            name: 'user',
            component: () => import('../views/UserView.vue'),
        },
    ],
})

router.beforeEach(async (to, _from, next) => {
    const auth = useAuth()
    await auth.inspectToken()

    const publicRoutes = new Set(['home', 'verification'])
    const targetName = to.name?.toString() ?? ''

    if (!auth.isLogin && !publicRoutes.has(targetName)) {
        next('/')
    } else {
        next()
    }
})

export default router
