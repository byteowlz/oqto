/**
 * React hook for the Oqto Event Bus.
 *
 * Usage:
 *   const bus = useBus();
 *   bus.publish({ scope: "session", scopeId: sessionId, topic: "app.message", payload: { ... } });
 *   bus.subscribe({ scope: "session", scopeId: sessionId, topics: ["app.*"] }, (event) => { ... });
 */

import { useCallback, useEffect, useRef } from "react";
import {
	type BusEventHandler,
	type BusSubscription,
	type PublishOptions,
	type SubscribeOptions,
	busPublish,
	busSubscribe,
} from "./bus-client";

export interface UseBusResult {
	publish: (opts: PublishOptions) => void;
	subscribe: (
		opts: SubscribeOptions,
		handler: BusEventHandler,
	) => BusSubscription;
}

/**
 * Hook that provides bus publish/subscribe with automatic cleanup on unmount.
 */
export function useBus(): UseBusResult {
	const subscriptionsRef = useRef<BusSubscription[]>([]);

	// Cleanup all subscriptions on unmount
	useEffect(() => {
		return () => {
			for (const sub of subscriptionsRef.current) {
				sub.unsubscribe();
			}
			subscriptionsRef.current = [];
		};
	}, []);

	const publish = useCallback((opts: PublishOptions) => {
		busPublish(opts);
	}, []);

	const subscribe = useCallback(
		(opts: SubscribeOptions, handler: BusEventHandler): BusSubscription => {
			const sub = busSubscribe(opts, handler);
			subscriptionsRef.current.push(sub);
			return {
				unsubscribe: () => {
					sub.unsubscribe();
					subscriptionsRef.current = subscriptionsRef.current.filter(
						(s) => s !== sub,
					);
				},
			};
		},
		[],
	);

	return { publish, subscribe };
}
