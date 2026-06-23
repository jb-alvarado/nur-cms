<template>
    <nav class="navbar bg-base-100 shadow-sm sticky top-0 z-20">
        <div class="navbar-start">
            <div class="dropdown">
                <div tabindex="0" role="button" class="btn btn-ghost lg:hidden">
                    <i class="bi bi-list text-2xl"></i>
                </div>
                <ul
                    tabindex="-1"
                    class="menu menu-sm dropdown-content bg-base-100 rounded-box z-1 mt-3 w-52 p-2 shadow"
                >
                    <li><RouterLink to="/">Home</RouterLink></li>
                    <li><a href="/#entries">Articles</a></li>
                    <li><a href="https://github.com/jb-alvarado/nur-cms" target="_blank">GitHub</a></li>
                </ul>
            </div>
            <RouterLink to="/" class="btn btn-ghost text-xl">NUR CMS Example</RouterLink>
        </div>
        <div class="navbar-end hidden lg:flex">
            <ul class="menu menu-horizontal px-1">
                <li><RouterLink to="/">Home</RouterLink></li>
                <li><a href="/#entries">Articles</a></li>
                <li><a href="https://github.com/jb-alvarado/nur-cms" target="_blank">GitHub</a></li>
            </ul>
        </div>
    </nav>
</template>
