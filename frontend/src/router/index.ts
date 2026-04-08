import { createRouter, createWebHistory } from 'vue-router'
import HomeView from '../views/HomeView.vue'

import { useAuth } from '@/stores/auth'
import { useIndex } from './../stores/index'

const router = createRouter({
    history: createWebHistory(import.meta.env.BASE_URL),
    routes: [
        {
            path: '/',
            name: 'home',
            component: HomeView,
        },
        {
            path: '/verification',
            name: 'verification',
            component: () => import('../views/VerificationView.vue'),
        },
        {
            path: '/author',
            name: 'author',
            component: () => import('../views/author/IndexView.vue'),
        },
        {
            path: '/author/:id',
            name: 'author edit',
            component: () => import('../views/author/EditView.vue'),
        },
        {
            path: '/author/:id/:group_id',
            name: 'group author edit',
            component: () => import('../views/author/EditView.vue'),
        },
        {
            path: '/category',
            name: 'category',
            component: () => import('../views/category/IndexView.vue'),
        },
        {
            path: '/category/:id',
            name: 'category edit',
            component: () => import('../views/category/EditView.vue'),
        },
        {
            path: '/category/:id/:group_id',
            name: 'group category edit',
            component: () => import('../views/category/EditView.vue'),
        },
        {
            path: '/configuration',
            name: 'configuration',
            component: () => import('../views/ConfigurationView.vue'),
        },
        {
            path: '/content/:type',
            name: 'content type',
            component: () => import('../views/content/IndexView.vue'),
        },
        {
            path: '/content/:type/:id',
            name: 'content edit',
            component: () => import('../views/content/EditView.vue'),
        },
        {
            path: '/content/:type/:id/:group_id',
            name: 'group content edit',
            component: () => import('../views/content/EditView.vue'),
        },
        {
            path: '/comment',
            name: 'comment',
            component: () => import('../views/comment/IndexView.vue'),
        },
        {
            path: '/comment/:id',
            name: 'comment edit',
            component: () => import('../views/comment/EditView.vue'),
        },
        {
            path: '/comment/:id/:group_id',
            name: 'group comment edit',
            component: () => import('../views/comment/EditView.vue'),
        },
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
        {
            path: '/:pathMatch(.*)*',
            name: '404',
            component: () => import('../views/404NotFount.vue'),
            alias: '/404',
        },
    ],
})

router.beforeEach(async (to, from) => {
    const auth = useAuth()
    const store = useIndex()
    await auth.inspectToken()

    if (
        (to.path.startsWith('/author') && !from.path.startsWith('/author')) ||
        (to.path.startsWith('/category') && !from.path.startsWith('/category')) ||
        (to.path.startsWith('/content') && !from.path.startsWith('/content')) ||
        (to.path.startsWith('/comment') && !from.path.startsWith('/comment')) ||
        (to.path.startsWith('/media') && !from.path.startsWith('/media'))
    ) {
        store.search = ''
    }

    const publicRoutes = new Set(['home', 'verification', '404'])
    const targetName = to.name?.toString() ?? ''

    if (!auth.isLogin && !publicRoutes.has(targetName)) {
        return '/'
    } else {
        return
    }
})

export default router
