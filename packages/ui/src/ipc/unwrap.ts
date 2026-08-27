export function unwrap<T>(
  promise: Promise<{ status: 'ok'; data: T } | { status: 'error'; error: unknown }>,
): Promise<T> {
  return promise.then((result) => {
    if (result.status === 'ok') return result.data;
    throw result.error;
  });
}
