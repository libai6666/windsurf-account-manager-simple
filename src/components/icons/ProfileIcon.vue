<template>
  <svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.6"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <!-- 后置窗口（只画露出的右上 L 形 + 顶部 title bar 右段） -->
    <path d="M8 4h11a1.5 1.5 0 0 1 1.5 1.5V16" />
    <path d="M14 8h6.5" />
    <!-- 前置窗口（完整轮廓） -->
    <rect x="3" y="8" width="13" height="12" rx="1.6" />
    <path d="M3 11.5h13" />
    <!-- 前窗口标题栏的两个圆点（窗口控件感） -->
    <circle cx="5.4" cy="9.75" r="0.55" fill="currentColor" stroke="none" />
    <circle cx="7.4" cy="9.75" r="0.55" fill="currentColor" stroke="none" />
  </svg>
</template>

<script setup lang="ts"></script>

<style scoped>
svg {
  width: 1em;
  height: 1em;
}
</style>
