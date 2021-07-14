# File Transfer over Clipboard (ftoc)

是个通过剪贴板来传输文件的小工具，例如在 VMWare Horizon 的客户机和宿主机之间实现文件传输

**当前仅支持 Windows**

## 使用

接收方先开启 ftoc：

```
ftoc
```

发送方：

```
ftoc <file>
```

就可以了。

## 发送参数设置

- --size (-s) `n`: 设置单个文件块大小，越大传输越快，支持大小后缀，例如 --size 1k / --size 2m 等,但正常剪贴板有大小限制，注意不要超过。否则文件传输会不完整

- --skip (-S) `n`: 断点续传专用，从第`n`个块开始传输

- --send-timeout (-st) `n`: 设置块之间的发送间隔，越小传输越快

## 接收参数设置

- --recv-timeout (-rt) `n`: 设置接收(剪贴板检测)超时，正常要比`--send-timeout`小
