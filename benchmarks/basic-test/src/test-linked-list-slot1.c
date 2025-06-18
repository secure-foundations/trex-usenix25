struct Node {
	struct Node* next;
	int data;
};

int getlast(struct Node* n) {
	struct Node* nxt = n->next;
	while(nxt != 0) {
		n = nxt;
		nxt = n->next;
	}
	return n->data;
}
