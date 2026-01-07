export interface FuzzyMatch {
    matches: boolean;
    score: number;
}
export declare function fuzzyMatch(query: string, text: string): FuzzyMatch;
export declare function fuzzyFilter<T>(items: T[], query: string, getText: (item: T) => string): T[];
//# sourceMappingURL=fuzzy.d.ts.map