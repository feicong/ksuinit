# Workflow重构任务

## 背景
两个workflow文件（build-kernelsu-ko.yml 和 build-ksuinit.yml）存在重复的KernelSU构建逻辑，需要抽象出共同逻辑到独立的可重用workflow。

## 执行计划

### 1. 创建可重用workflow：build-kernelsu-common.yml
- 支持参数：kmi_matrix、ddk_release、kernelsu_tag
- 包含完整的KernelSU构建逻辑

### 2. 重构build-kernelsu-ko.yml 
- 调用可重用workflow
- 保留collect-artifacts作业

### 3. 重构build-ksuinit.yml
- 调用可重用workflow 
- 保留ksuinit构建和发布逻辑

## 参数配置
- kmi_matrix: KMI版本JSON数组
- ddk_release: "20250825"
- kernelsu_tag: 可选，默认"main"
