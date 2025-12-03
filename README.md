<!DOCTYPE html>

<html lang="zh-CN">
<head>
<meta charset="UTF-8">

### BlueStarOS - 一个用 Rust 开发的轻量级操作系统内核



</head>

<body>

<header>

<h1>BlueStarOS</h1>

<div class="badges">

<img src="https://img.shields.io/badge/rust-latest-brightgreen" alt="Rust" class="badge">

<img src="https://img.shields.io/badge/license-MIT-blue" alt="License" class="badge">

<img src="https://img.shields.io/badge/OS%20Kernel-Learning%20Project-orange" alt="Type" class="badge">

<img src="https://img.shields.io/badge/status-active-success" alt="Status" class="badge">

</div>

</header>


<nav>
    <h2>目录</h2>
    <ul>
        <li><a href="#注意">注意</a></li>
        <li><a href="#简介">简介</a></li>
        <li><a href="#功能特性">功能特性</a></li>
        <li><a href="#快速开始">快速开始</a></li>
        <li><a href="#使用说明">使用说明</a></li>
        <li><a href="#开发计划">开发计划</a></li>
        <li><a href="#许可证">许可证</a></li>
        <li><a href="#致谢">致谢</a></li>
        <li><a href="#联系方式">联系方式</a></li>
    </ul>
</nav>

<section id="注意">
    <h1>本项目处于开发阶段，模块和功能尚不完善！</h1>
</section>

<section id="简介">
    <h2>简介</h2>
    <p>BlueStarOS 是一个使用 Rust 语言开发的轻量级操作系统内核。该内核是本人学习操作系统过程中实践的产物，开发过程中部分构架和设计参考主流教学操作系统rCore及xv6</p>
</section>

<section id="功能特性">
    <h2>功能特性</h2>
    <ul>
        <li><strong>动态堆分配器</strong>: 实现基于 Buddy System 或类似算法的动态内存管理</li>
        <li><strong>陷阱处理程序</strong>: 处理异常、中断和系统调用陷阱</li>
        <li><strong>虚拟地址空间和分页功能</strong>: 实现虚拟内存管理，支持分页机制</li>
        <li><strong>任务切换功能</strong>:已经实现基于时钟中断的Stride任务调度算法支持多用户程序运行，syscall目前较完善</li>
        <li><strong>下一步开发目标</strong>: 完善用户库，支持基本系统调用，开发文件系统和硬件驱动</li>
    </ul>
</section>

<section id="快速开始">
    <h2>快速开始</h2>
    
    前置依赖
    在开始之前，请确保您的系统已安装以下软件：
    
        Rust工具链（1.6nightly版本）
        QEMU（ 7.0 版本）
        git
        GNU Make
    
  安装与运行
    
        克隆仓库：
        git clone https://github.com/Dirinkbottle/BlueStarOS.git
        cd BlueStarOS

</li>

<li>构建项目：

<pre><code>cargo build --release</code></pre>

</li>

<li>运行内核（使用 QEMU）：

<pre><code>make run LOG=TRACE</code></pre>

</li>

</ol>

<p>注意：实际运行命令可能因架构和配置不同而有所调整，请参考项目内的具体文档。
</p>

</section>

复制
<section id="使用说明">
    <h2>使用说明</h2>
    <p>当前版本的 BlueStarOS 主要提供内核不完整功能演示。运行时，内核将初始化内存分配器和初始化内核地址空间 </p>
    <p>您可以通过查看串口输出或控制台信息来观察内核的运行状态和调试信息。</p>
</section>

<section id="开发计划">
    <h2>开发计划</h2>
    <ul>
        <li><strong>中期目标</strong>: 完成文件系统开发，完成usb，串口等硬件驱动开发</li>
        <li><strong>长期目标</strong>: 支持shell，命令行</li>
        <li><strong>最终目标</strong>: 开发图形api，显卡驱动，支持图形化桌面</li>
    </ul>
</section>

<section id="许可证">
    <h2>许可证</h2>
    <p>本项目基于 MIT 许可证开源。详情请查看 <a href="LICENSE">LICENSE</a> 文件。</p>
</section>

<section id="致谢">
    <h2>致谢</h2>
    <p>感谢以下资源对项目的启发和帮助：</p>
    <ul>
        <li><a href="https://www.rust-lang.org/">Rust 编程语言社区</a></li>
        <li><a href="https://opencamp.cn/os2edu/camp/2025fall">2025冬季开源操作系统训练营</a></li>
        <li>所有为操作系统开发提供优秀文档的开发者</li>
    </ul>
</section>

<section id="联系方式">
    <h2>联系方式</h2>
    <p>如有问题或建议，请通过以下方式联系：</p>
    <ul>
        <li>项目维护者邮箱：yellowfish@dirinkbottle.asia</li>
        <li>GitHub 仓库：<a href="https://github.com/Dirinkbottle/BlueStarOS/">BlueStarOS</a></li>
        <li>项目议题页面：<a href="https://github.com/Dirinkbottle/BlueStarOS/issues">提交问题或建议</a></li>
    </ul>
</section>
<footer>
    <p>© 2025 BlueStarOS By Dirinkbottle</p>
</footer>
</body>

</html>

