import {
	type Dispatch,
	type SetStateAction,
	useCallback,
	useEffect,
	useState,
} from "react";

type InitialValue<T> = T | (() => T);

interface UseLocalStorageOptions<T> {
	serialize?: (value: T) => string;
	deserialize?: (raw: string) => T;
	onError?: (error: unknown) => void;
}

function resolveInitialValue<T>(initialValue: InitialValue<T>): T {
	return typeof initialValue === "function"
		? (initialValue as () => T)()
		: initialValue;
}

/**
 * LocalStorage-backed state with SSR-safe initialization and cross-tab sync.
 */
export function useLocalStorage<T>(
	key: string,
	initialValue: InitialValue<T>,
	options: UseLocalStorageOptions<T> = {},
): [T, Dispatch<SetStateAction<T>>, () => void] {
	const {
		serialize = JSON.stringify,
		deserialize = JSON.parse as (raw: string) => T,
		onError,
	} = options;

	const [storedValue, setStoredValue] = useState<T>(() => {
		const fallback = resolveInitialValue(initialValue);
		if (typeof window === "undefined") {
			return fallback;
		}

		try {
			const raw = window.localStorage.getItem(key);
			if (raw === null) {
				return fallback;
			}
			return deserialize(raw);
		} catch (error) {
			onError?.(error);
			return fallback;
		}
	});

	useEffect(() => {
		if (typeof window === "undefined") return;
		try {
			window.localStorage.setItem(key, serialize(storedValue));
		} catch (error) {
			onError?.(error);
		}
	}, [key, onError, serialize, storedValue]);

	useEffect(() => {
		if (typeof window === "undefined") return;
		const handleStorage = (event: StorageEvent) => {
			if (event.key !== key || event.newValue === null) return;
			try {
				setStoredValue(deserialize(event.newValue));
			} catch (error) {
				onError?.(error);
			}
		};

		window.addEventListener("storage", handleStorage);
		return () => window.removeEventListener("storage", handleStorage);
	}, [deserialize, key, onError]);

	const removeValue = useCallback(() => {
		if (typeof window === "undefined") return;
		try {
			window.localStorage.removeItem(key);
		} catch (error) {
			onError?.(error);
		}
		setStoredValue(resolveInitialValue(initialValue));
	}, [initialValue, key, onError]);

	return [storedValue, setStoredValue, removeValue];
}
