import {
	type InfiniteData,
	type UseInfiniteQueryResult,
	useInfiniteQuery,
} from "@tanstack/react-query";
import { useMemo } from "react";
import type {
	ChatMessage,
	MessagePage,
	OqtoUiPlatform,
} from "../platform/contracts";

const TIMELINE_PAGE_SIZE = 200;

export type Timeline = {
	/** All loaded messages, oldest-first. */
	messages: ChatMessage[];
	/** True when older messages exist that are not loaded yet. */
	hasMore: boolean;
	/** True while an older page is being fetched. */
	loadingEarlier: boolean;
	/** Fetch the next-older page. */
	loadEarlier: () => void;
	/** Refetch from the newest page. */
	reload: () => void;
};

type TimelinePageParam = string | undefined;

function selectOldestCursor(lastPage: MessagePage): TimelinePageParam {
	return lastPage.hasMore && lastPage.nextBefore
		? lastPage.nextBefore
		: undefined;
}

/**
 * Per-View timeline ownership: each Chat View runs its own query keyed by
 * Session id, loading the newest page first and fetching earlier pages on
 * demand. Pages reconcile by the durable cursor position, never by index or
 * count.
 */
export function useTimeline(
	platform: Pick<OqtoUiPlatform, "id" | "loadMessages">,
	sessionId: string,
): Timeline {
	const query: UseInfiniteQueryResult<
		InfiniteData<MessagePage, TimelinePageParam>,
		Error
	> = useInfiniteQuery({
		queryKey: ["oqto-ui", "timeline", platform.id, sessionId],
		queryFn: ({ pageParam }) =>
			platform.loadMessages(sessionId, pageParam, TIMELINE_PAGE_SIZE),
		initialPageParam: undefined as TimelinePageParam,
		getNextPageParam: selectOldestCursor,
		// Older pages are loaded deliberately, never on mount.
		maxPages: 32,
		staleTime: 30_000,
	});

	// Stable identity across renders: consumers memoize on this array.
	const messages = useMemo(
		() => (query.data?.pages ?? []).flatMap((page) => page.messages),
		[query.data],
	);
	const pages = query.data?.pages ?? [];
	const oldest = pages[0];

	return {
		messages,
		hasMore: oldest?.hasMore ?? false,
		loadingEarlier: query.isFetchingNextPage,
		loadEarlier: () => {
			if (!query.isFetchingNextPage && query.hasNextPage) {
				void query.fetchNextPage();
			}
		},
		reload: () => {
			void query.refetch();
		},
	};
}
