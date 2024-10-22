/** @format */

export const refactorBookmarks = (bookmarks) => {
  return bookmarks.map((bookmark) => ({
    id: bookmark.at(0),
    name: bookmark.at(1),
    link: bookmark.at(2),
    description: bookmark.at(3),
  }));
};
