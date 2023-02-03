# 特性

本文档概述了 Alacritty 在终端模拟能力之外的特性。要获取关于支持的控件序列的列表 请参照 [Alacritty's escape sequence support](./escape_support.md).

## Vi Mode

Vi 模式允许使用键盘控制 Alacritty 的可视区域移动与回滚。它是其他功能的基础，如搜索和使用键盘打开网址。默认情况下，您可以使用<kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>Space</kbd> 来开启。

### 光标移动

默认情况下，光标移动的按键绑定被设置为模仿Vi，但它们是完全可自定义配置的。如果您不喜欢 Vi 的绑定，参照[配置文件]来更改各种移动的按键绑定。

### 选择

Vi 模式的一个有用特性是能够进行选择并将文本复制到剪贴板。 默认情况下您可以通过按下 <kbd>v</kbd> 开始选择文本并通过按下<kbd>y</kbd>进行复制。鼠标可用的所有选择模式都可以
可从 Vi 模式实现, 包含语义选择 (<kbd>Alt</kbd> <kbd>v</kbd>) 、行选择 (<kbd>Shift</kbd> <kbd>v</kbd>) 和 块选择 (<kbd>Ctrl</kbd> <kbd>v</kbd>)。 您还可以在仍处于选择状态时在它们之间切换。

## 搜索

搜索允许您在 Alacritty 的回滚缓冲区中找到任何内容。您可以使用 <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>f</kbd> 向前搜索和使用 <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>b</kbd> 向后搜索。

### Vi 搜索

在 Vi 模式中， 向前搜索为 <kbd>/</kbd>， 向后搜索为<kbd>?</kbd>。 这使您可以快速移动并辅助选择
内容。如果您正在寻找跳转到搜索匹配开始或结束的方法，则可以为 `SearchStart` 和 `SearchEnd` 方法绑定键操作。

### Normal模式搜索

在Normal模式搜索时，您不能自由移动光标，但您仍然可以使用<kbd>Enter</kbd> 和 <kbd>Shift</kbd>
<kbd>Enter</kbd> 在搜索结果间跳转， 使用 <kbd>Escape</kbd> 退出搜索后，您的活跃匹配项将保持选中状态，以便您轻松复制它。

## Hints

终端 Hints 允许轻松与可见文本交互，而无需启动 Vi 模式。它们由一个正则表达式组成，该正则表达式检测这些文本元素，然后将它们馈送到外部应用程序或触发 Alacritty 的内置操作之一。

Hints 可以使用鼠标或 Vi 模式光标触发。如果为一个 Hints 启用了鼠标交互，并被识别到，则当鼠标或 Vi 模式光标位于其顶部时，该 Hints 将带有下划线。使用鼠标左键或
在 Vi 模式下 <kbd>Enter</kbd> 键将触发 Hints。

Hints 可以在 Alacritty 的配置文件中的 `hints` 和 `colors.hints` 部分进行配置。

## 选择扩展

在进行选择后，单击鼠标右键会扩展您的选择直到单击位置。
双击鼠标右键则会根据语义扩展您的选择, 三击鼠标右键则会扩展到行选择。如果您在扩展选择时按住 <kbd>Ctrl</kbd> ，则会切换到块选择模式。

## 通过鼠标打开 URL 链接

您可以通过单击 URL 来用鼠标打开链接。需要保存的修饰符和应打开链接的程序都可以在配置文件中设置。如果某个应用程序捕获了鼠标单击（由鼠标光标形状的更改指示），则需要按住 <kbd>Shift</kbd> 才能绕过该操作。

[配置文件]: ../alacritty.yml

## 多窗口

Alacritty 支持从同一个 Alacritty 实例运行多个终端模拟器。可以使用 `CreateNewWindow` 键绑定或通过执行`alacritty msg create-window` 子命令来创建新窗口。
