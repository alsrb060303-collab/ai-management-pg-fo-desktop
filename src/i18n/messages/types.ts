/** English strings for an area: exactly the keys of its Korean strings. */
export type Messages<K> = { [P in keyof K]: string };
