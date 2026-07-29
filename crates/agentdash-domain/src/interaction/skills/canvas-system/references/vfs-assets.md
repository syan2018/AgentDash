# Canvas VFS 图片资源

浏览器不能直接加载 VFS URI。使用 host asset broker：

```ts
const url = await window.agentdash.assets.url("main://docs/diagram.png");
image.src = url;

// 不再使用时
await window.agentdash.assets.revoke(url);
```

## 规则

- URI 为 `<mount_id>://<mount_relative_path>`。
- 只解析当前 Canvas actor/attachment 的 applied resource surface。
- 当前实现只返回图片 MIME 的 revocable object URL；文本数据使用 ResourceSlot binding
  materialize 到 `bindings/` 后导入。
- attachment、surface 或 renderer generation 失效后，不复用旧 URL。
- reload/unmount 自动释放；组件提前结束使用时主动 revoke。
- 不把 `surface_ref`、backend ID、认证 header、signed provider URL 或本机路径传进 iframe。
