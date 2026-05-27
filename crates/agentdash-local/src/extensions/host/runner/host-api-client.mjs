export function createHostApiClient({
  send,
  toJsonValue,
  invocationContextParams,
}) {
  let nextHostApiId = 1;
  const pendingHostApi = new Map();

  async function requestHostApi(method, params, extensionKey) {
    const id = `host-api-${nextHostApiId++}`;
    send({
      kind: "host_api_request",
      id,
      method,
      params: toJsonValue({ ...invocationContextParams(extensionKey), ...params }),
    });
    return await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        pendingHostApi.delete(id);
        reject(new Error(`host api timeout: ${method}`));
      }, 30000);
      pendingHostApi.set(id, {
        resolve(value) {
          clearTimeout(timeout);
          resolve(value);
        },
        reject(error) {
          clearTimeout(timeout);
          reject(error);
        },
      });
    });
  }

  function handleHostApiResponse(message) {
    const pending = pendingHostApi.get(message.id);
    if (!pending) return;
    pendingHostApi.delete(message.id);
    if (message.error) pending.reject(new Error(message.error));
    else pending.resolve(message.result ?? null);
  }

  return {
    requestHostApi,
    handleHostApiResponse,
  };
}
