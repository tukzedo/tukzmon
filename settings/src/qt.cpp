// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

// The settings window in Qt Widgets. Rows come from settings.shcl through the
// Rust side, and every change goes back there, so this file only lays out
// controls and copies state onto them. Same model as SilkTerm's dialog: tabs
// down the left, a revert arrow per row, and Cancel, Apply, OK.

#include "tukzmon-settings/src/qt.h"
#include "tukzmon-settings/src/bridge.rs.h"

#include <QApplication>
#include <QButtonGroup>
#include <QCheckBox>
#include <QComboBox>
#include <QCursor>
#include <QDialog>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QFrame>
#include <QKeyEvent>
#include <QGridLayout>
#include <QImage>
#include <QHBoxLayout>
#include <QInputDialog>
#include <QLabel>
#include <QLineEdit>
#include <QLinearGradient>
#include <QListWidget>
#include <QMessageBox>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPixmap>
#include <QProxyStyle>
#include <QPushButton>
#include <QRadioButton>
#include <QScreen>
#include <QRegularExpressionValidator>
#include <QScrollArea>
#include <QSignalBlocker>
#include <QSlider>
#include <QSpinBox>
#include <QStackedWidget>
#include <QStyle>
#include <QTimer>
#include <QToolButton>
#include <QToolTip>
#include <QVBoxLayout>

#include <algorithm>
#include <cmath>
#include <functional>
#include <string>
#include <tuple>
#include <memory>
#include <vector>

namespace tukzmon {
namespace {

QString qs(const rust::String &s) { return QString::fromUtf8(s.data(), int(s.size())); }

// The style's own selection is a solid block meant for highlighted text, and in
// a dark theme that's white on bright blue. A tint of it under the plain text
// color reads in a light theme and a dark one.
void softenSelection(QWidget *w) {
	QPalette pal = w->palette();
	for (auto group : {QPalette::Active, QPalette::Inactive}) {
		const QColor base = pal.color(group, QPalette::Base);
		const QColor pick = pal.color(group, QPalette::Highlight);
		auto mix = [](float a, float b) { return a + (b - a) * 0.35f; };
		const QColor tint = QColor::fromRgbF(mix(base.redF(), pick.redF()), mix(base.greenF(), pick.greenF()),
			mix(base.blueF(), pick.blueF()));
		pal.setColor(group, QPalette::Highlight, tint);
		pal.setColor(group, QPalette::HighlightedText, pal.color(group, QPalette::Text));
	}
	w->setPalette(pal);
}

QString wrapped(const QString &tip) {
	const QByteArray u = tip.toUtf8();
	return qs(wrap_tip(rust::Str(u.constData(), size_t(u.size()))));
}

// Qt shows a plain text tip as one line however long, so every tip in the
// window goes through here first.
class TipWrap : public QObject {
protected:
	bool eventFilter(QObject *o, QEvent *e) override {
		if (e->type() != QEvent::ToolTip || !o->isWidgetType())
			return false;
		auto *w = static_cast<QWidget *>(o);
		// none of its own, so the widget passes it on to its parent
		if (w->toolTip().isEmpty())
			return false;
		QToolTip::showText(static_cast<QHelpEvent *>(e)->globalPos(), wrapped(w->toolTip()), w, QRect(),
			w->toolTipDuration());
		return true;
	}
};

// Some dark themes draw an empty checkbox with an edge close to the window
// color, so it all but vanishes. An edge in the text color, a little faded,
// shows in a light theme and a dark one.
class BoxStyle : public QProxyStyle {
public:
	void drawPrimitive(PrimitiveElement pe, const QStyleOption *opt, QPainter *p, const QWidget *w) const override {
		QProxyStyle::drawPrimitive(pe, opt, p, w);
		if (pe != PE_IndicatorCheckBox && pe != PE_IndicatorRadioButton)
			return;
		QColor edge = opt->palette.color(QPalette::WindowText);
		edge.setAlphaF(0.6f);
		p->save();
		p->setRenderHint(QPainter::Antialiasing);
		p->setPen(QPen(edge, 1));
		p->setBrush(Qt::NoBrush);
		const QRectF r = QRectF(opt->rect).adjusted(0.5, 0.5, -0.5, -0.5);
		if (pe == PE_IndicatorRadioButton)
			p->drawEllipse(r);
		else
			p->drawRoundedRect(r, 2, 2);
		p->restore();
	}
};

constexpr int SLIDER_STEPS = 1000;
constexpr int HEAD_GAP = 22;
constexpr int BUTTON_GROW = 8;

// QScrollArea's own hint stops at about 24 lines, so a tall tab opened with a
// scroll bar it had no need for.
class FitScroll : public QScrollArea {
public:
	QSize sizeHint() const override {
		QSize s = QScrollArea::sizeHint();
		if (widget())
			s.setHeight(std::max(s.height(), widget()->sizeHint().height() + 2 * frameWidth()));
		return s;
	}
};

// A press or drag on the square or the strip, or an arrow key on it.
class PickArea : public QWidget {
public:
	std::function<void(double, double)> moved;
	double h = 0, s = 0, v = 0;

protected:
	// x and y are 0 to 1 across the area, clamped, so a drag off the edge holds it there
	virtual void pickAt(double x, double y) = 0;
	virtual void arrow(int dx, int dy, double by) = 0;

	void mousePressEvent(QMouseEvent *e) override {
		if (e->button() == Qt::LeftButton)
			pickFrom(e);
	}
	void mouseMoveEvent(QMouseEvent *e) override {
		if (e->buttons() & Qt::LeftButton)
			pickFrom(e);
	}
	void keyPressEvent(QKeyEvent *e) override {
		const double by = e->modifiers() & Qt::ShiftModifier ? 0.1 : 0.01;
		switch (e->key()) {
		case Qt::Key_Left: arrow(-1, 0, by); break;
		case Qt::Key_Right: arrow(1, 0, by); break;
		case Qt::Key_Up: arrow(0, -1, by); break;
		case Qt::Key_Down: arrow(0, 1, by); break;
		default: QWidget::keyPressEvent(e);
		}
	}
	void edge(QPainter &p) {
		const bool on = hasFocus();
		p.setPen(QPen(palette().color(on ? QPalette::Highlight : QPalette::Mid), on ? 2 : 1));
		p.setBrush(Qt::NoBrush);
		p.drawRect(QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5));
	}

private:
	void pickFrom(QMouseEvent *e) {
		const QPointF at = e->position();
		pickAt(std::clamp(at.x() / std::max(1, width() - 1), 0.0, 1.0),
			std::clamp(at.y() / std::max(1, height() - 1), 0.0, 1.0));
	}
};

// saturation left to right, brightness bottom to top, at one hue
class SvSquare : public PickArea {
public:
	explicit SvSquare(int side) {
		setFixedSize(side, side);
		setFocusPolicy(Qt::StrongFocus);
		setCursor(Qt::CrossCursor);
	}

protected:
	void pickAt(double x, double y) override { moved(x, 1 - y); }
	void arrow(int dx, int dy, double by) override {
		moved(std::clamp(s + dx * by, 0.0, 1.0), std::clamp(v - dy * by, 0.0, 1.0));
	}
	void paintEvent(QPaintEvent *) override {
		QPainter p(this);
		p.fillRect(rect(), QColor::fromHsvF(float(h), 1, 1));
		QLinearGradient white(0, 0, width(), 0);
		white.setColorAt(0, QColor(255, 255, 255));
		white.setColorAt(1, QColor(255, 255, 255, 0));
		p.fillRect(rect(), white);
		QLinearGradient black(0, 0, 0, height());
		black.setColorAt(0, QColor(0, 0, 0, 0));
		black.setColorAt(1, QColor(0, 0, 0));
		p.fillRect(rect(), black);
		edge(p);
		const QColor now = QColor::fromHsvF(float(h), float(s), float(v));
		const bool light = 0.2126 * now.redF() + 0.7152 * now.greenF() + 0.0722 * now.blueF() > 0.5;
		p.setRenderHint(QPainter::Antialiasing);
		p.setPen(QPen(light ? Qt::black : Qt::white, 2));
		p.drawEllipse(QPointF(s * (width() - 1), (1 - v) * (height() - 1)), 5, 5);
	}
};

// the hue, top to bottom
class HueStrip : public PickArea {
public:
	HueStrip(int w, int h) {
		setFixedSize(w, h);
		setFocusPolicy(Qt::StrongFocus);
	}

protected:
	void pickAt(double, double y) override { moved(y, 0); }
	void arrow(int dx, int dy, double by) override {
		if (dy)
			moved(std::fmod(h + dy * by + 1, 1), 0);
		else
			focusNextPrevChild(dx > 0);
	}
	void paintEvent(QPaintEvent *) override {
		QPainter p(this);
		QLinearGradient ramp(0, 0, 0, height());
		for (int k = 0; k <= 6; k++)
			ramp.setColorAt(k / 6.0, QColor::fromHsvF(k == 6 ? 0 : k / 6.0f, 1, 1));
		p.fillRect(rect(), ramp);
		edge(p);
		// the strip is every hue, so its marker is black and white
		const double y = h * (height() - 1);
		p.fillRect(QRectF(0, y - 2.5, width(), 5), Qt::black);
		p.fillRect(QRectF(1, y - 1.5, width() - 2, 3), Qt::white);
	}
};

// SilkTerm's picker, not the toolkit's. The row behind follows every change,
// and Cancel puts back all of it.
class ColorPicker : public QDialog {
public:
	ColorPicker(Dialog &d, const QString &title, std::function<void()> changed, QWidget *parent)
		: QDialog(parent), d(d), changed(std::move(changed)) {
		setWindowTitle(title);
		const int line = fontMetrics().height();
		square = new SvSquare(line * 12);
		strip = new HueStrip(line * 3 / 2, line * 12);
		square->moved = [this](double s, double v) { set(state.h, s, v); };
		strip->moved = [this](double h, double) { set(h, state.s, state.v); };

		state = pick_state(d);
		auto *form = new QFormLayout;
		for (size_t i = 0; i < state.fields.size(); i++) {
			const bool hex = i + 1 == state.fields.size();
			QWidget *box = nullptr;
			if (hex) {
				auto *edit = new QLineEdit;
				edit->setValidator(new QRegularExpressionValidator(QRegularExpression("#?[0-9a-fA-F]{0,6}"), edit));
				edit->setMaximumWidth(edit->fontMetrics().horizontalAdvance("#DDDDDDD") + 16);
				connect(edit, &QLineEdit::textEdited, this, [this, i, edit](const QString &t) {
					if (pick_type(this->d, i, t.toStdString()))
						sync(edit);
				});
				// a half-typed hex shows the color again on the way out
				connect(edit, &QLineEdit::editingFinished, this, [this] { sync(nullptr); });
				hexBox = edit;
				box = edit;
			} else {
				auto *spin = new QSpinBox;
				spin->setRange(0, 100);
				connect(spin, &QSpinBox::valueChanged, this, [this, i, spin](int v) {
					if (pick_type(this->d, i, std::to_string(v)))
						sync(spin);
				});
				spins.push_back(spin);
				box = spin;
			}
			form->addRow(qs(state.labels[i]), box);
		}

		auto *cancel = new QPushButton("Cancel");
		auto *ok = new QPushButton("OK");
		cancel->setAutoDefault(false);
		ok->setDefault(true);
		connect(cancel, &QPushButton::clicked, this, &QDialog::reject);
		connect(ok, &QPushButton::clicked, this, &QDialog::accept);
		auto *buttons = new QHBoxLayout;
		buttons->addStretch(1);
		buttons->addWidget(cancel);
		buttons->addWidget(ok);

		auto *top = new QHBoxLayout;
		top->addWidget(square, 0, Qt::AlignTop);
		top->addWidget(strip, 0, Qt::AlignTop);
		top->addSpacing(line);
		top->addLayout(form);
		auto *outer = new QVBoxLayout(this);
		outer->addLayout(top);
		outer->addLayout(buttons);
		sync(nullptr);
		square->setFocus();
	}

	void done(int r) override {
		pick_close(d, r == QDialog::Accepted);
		changed();
		QDialog::done(r);
	}

private:
	Dialog &d;
	std::function<void()> changed;
	PickState state;
	SvSquare *square = nullptr;
	HueStrip *strip = nullptr;
	std::vector<QSpinBox *> spins;
	QLineEdit *hexBox = nullptr;

	void set(double h, double s, double v) {
		pick_set(d, h, s, v);
		sync(nullptr);
	}

	// `typing` is the box being typed in, which keeps its own text
	void sync(QWidget *typing) {
		state = pick_state(d);
		for (PickArea *a : {static_cast<PickArea *>(square), static_cast<PickArea *>(strip)}) {
			a->h = state.h;
			a->s = state.s;
			a->v = state.v;
			a->update();
		}
		for (size_t i = 0; i < spins.size(); i++) {
			if (spins[i] == typing)
				continue;
			QSignalBlocker b(spins[i]);
			spins[i]->setValue(std::stoi(std::string(state.fields[i])));
		}
		if (hexBox != typing) {
			QSignalBlocker b(hexBox);
			hexBox->setText(qs(state.hex));
		}
		changed();
	}
};

// The handle at the right end of a graph list row. Dragging it moves the row,
// and so do Up and Down while it has focus.
class Gripper : public QWidget {
public:
	std::function<void(QPoint)> dragged;
	std::function<void(int)> stepped;

	Gripper() {
		setFocusPolicy(Qt::StrongFocus);
		setCursor(Qt::OpenHandCursor);
		const int line = fontMetrics().height();
		setFixedSize(line, line * 3 / 2);
	}

protected:
	void mousePressEvent(QMouseEvent *e) override {
		if (e->button() != Qt::LeftButton)
			return;
		setCursor(Qt::ClosedHandCursor);
		setFocus(Qt::MouseFocusReason);
	}
	void mouseMoveEvent(QMouseEvent *e) override {
		if (e->buttons() & Qt::LeftButton)
			dragged(e->globalPosition().toPoint());
	}
	void mouseReleaseEvent(QMouseEvent *) override { setCursor(Qt::OpenHandCursor); }
	void keyPressEvent(QKeyEvent *e) override {
		switch (e->key()) {
		case Qt::Key_Up: stepped(-1); break;
		case Qt::Key_Down: stepped(1); break;
		default: QWidget::keyPressEvent(e);
		}
	}
	void focusInEvent(QFocusEvent *) override { update(); }
	void focusOutEvent(QFocusEvent *) override { update(); }
	// 2 columns of 3 dots, the usual drag handle
	void paintEvent(QPaintEvent *) override {
		QPainter p(this);
		p.setRenderHint(QPainter::Antialiasing);
		p.setPen(Qt::NoPen);
		// Mid all but vanishes in some light styles
		QColor ink = palette().color(QPalette::WindowText);
		ink.setAlphaF(0.45f);
		p.setBrush(hasFocus() ? palette().color(QPalette::Highlight) : ink);
		const double dot = std::max(2.0, width() / 6.0);
		const QPointF mid(width() / 2.0, height() / 2.0);
		for (int col : {-1, 1})
			for (int row : {-1, 0, 1})
				p.drawEllipse(mid + QPointF(col * dot, row * dot * 2), dot / 2, dot / 2);
	}
};

// A graph list row's graph, read and drawn here with the window's look, so a
// change shows before Apply.
class LiveGraph : public QWidget {
public:
	LiveGraph(Live &live, std::string name) : live(live), name(std::move(name)) {
		setFixedSize(0, fontMetrics().height() * 3 / 2);
		setMouseTracking(true);
	}

	// The graph's own width, as in the panel. A very wide one is squeezed to
	// fit the window, so its whole time span still shows.
	void fit() {
		own = int(live_width(live, name));
		const int most = 512;
		setFixedWidth(std::min(own, most));
		update();
	}

	// after a read, so a tip left up keeps up, as the panel's does
	void tick() {
		update();
		if (underMouse() && QToolTip::isVisible())
			showTip(QCursor::pos());
	}

protected:
	void paintEvent(QPaintEvent *) override {
		const auto [w, h, scale] = drawn();
		if (w <= 0 || h <= 0)
			return;
		auto px = live_draw(live, name, size_t(w), size_t(h), size_t(scale));
		if (px.size() != size_t(w) * size_t(h))
			return;
		// cairo's ARGB32 is Qt's premultiplied one
		QImage img(reinterpret_cast<const uchar *>(px.data()), w, h, w * 4, QImage::Format_ARGB32_Premultiplied);
		QPainter p(this);
		p.drawImage(rect(), img);
		overlay(p, size_t(w), size_t(h), scale);
		if (!framePending && live_easing(live, name)) {
			framePending = true;
			QTimer::singleShot(33, this, [this] {
				framePending = false;
				update();
			});
		}
	}
	void mouseMoveEvent(QMouseEvent *e) override {
		point(e->position().x());
		if (QToolTip::isVisible())
			showTip(e->globalPosition().toPoint());
	}
	void leaveEvent(QEvent *) override {
		if (live_leave(live))
			update();
	}
	bool event(QEvent *e) override {
		if (e->type() != QEvent::ToolTip)
			return QWidget::event(e);
		auto *help = static_cast<QHelpEvent *>(e);
		point(help->pos().x());
		showTip(help->globalPos());
		return true;
	}

private:
	// device pixels, as drawn before a squeeze to fit the window
	// as the panel writes it, in the window's font, with a rim. The image may
	// be squeezed into the row, but the text isn't.
	void overlay(QPainter &p, size_t w, size_t h, int scale) const {
		const OverlayInfo o = live_overlay(live, name, w, h, size_t(scale));
		if (!o.shown)
			return;
		const QString text = QString::fromStdString(std::string(o.text));
		const QFontMetrics fm = fontMetrics();
		const double fx = double(width()) / double(w), fy = double(height()) / double(h);
		const double pad = 1;
		if (fm.horizontalAdvance(text) + 2 * pad > double(o.w) * fx || fm.height() > double(o.h) * fy)
			return;
		QPainterPath path;
		path.addText(double(o.x) * fx + pad, double(o.y) * fy + fm.ascent(), font(), text);
		p.setRenderHint(QPainter::Antialiasing);
		p.strokePath(path, QPen(QColor::fromRgb(o.edge), 2));
		p.fillPath(path, QColor::fromRgb(o.color));
	}

	std::tuple<int, int, int> drawn() const {
		const double dpr = devicePixelRatioF();
		return {int(std::lround(own * dpr)), int(std::lround(height() * dpr)), int(std::max(1L, std::lround(dpr)))};
	}
	// the hover line goes where the pointer is in the drawn graph, squeezed or not
	void point(double x) {
		const int w = std::get<0>(drawn());
		if (w <= 0 || width() <= 0)
			return;
		const auto col = size_t(std::max(0.0, std::floor(x * w / width())));
		if (live_hover(live, name, col, size_t(w)))
			update();
	}
	void showTip(QPoint at) {
		const auto [w, h, scale] = drawn();
		QToolTip::showText(at, qs(wrap_tip(live_tip(live, name, size_t(w), size_t(h), size_t(scale)))), this);
	}

	Live &live;
	std::string name;
	int own = 0;
	bool framePending = false;
};

struct Row {
	RowInfo info;
	QLabel *label = nullptr;
	QCheckBox *check = nullptr;
	QSlider *slider = nullptr;
	QDoubleSpinBox *spin = nullptr;
	QComboBox *combo = nullptr;
	QLineEdit *line = nullptr;
	QPushButton *chip = nullptr;
	QToolButton *revert = nullptr;
	QWidget *control = nullptr;
	QLabel *head = nullptr;
	QFrame *rule = nullptr;
	QGridLayout *grid = nullptr;
	int gap = -1;
	std::vector<QRadioButton *> radios;
	// what a dropdown's items stand for, which can change as themes are saved
	std::vector<std::string> keys;
	std::string id() const { return std::string(info.id); }
};

class Window : public QDialog {
public:
	explicit Window(Dialog &d) : d(d), live(live_new(d)) {
		setWindowTitle(qs(dialog_title(d)));
		auto tabs = dialog_tabs(d);
		for (auto &r : dialog_rows(d)) {
			auto row = std::make_unique<Row>();
			row->info = r;
			rows.push_back(std::move(row));
		}

		tabList = new QListWidget;
		softenSelection(tabList);
		pages = new QStackedWidget;
		auto hasRows = [this](int tab) {
			return std::any_of(rows.begin(), rows.end(), [tab](auto &r) { return r->info.tab == tab; });
		};
		// knobs lists a parent before its tabs, so one pass finds every stack
		std::vector<QStackedWidget *> stackOf(tabs.size(), nullptr);
		for (size_t i = 0; i < tabs.size(); i++) {
			bool parent = false;
			for (auto &t : tabs)
				parent |= t.parent == int(i);
			int depth = 0;
			for (int p = tabs[i].parent; p >= 0; p = tabs[size_t(p)].parent)
				depth++;
			tabList->addItem(new QListWidgetItem(QString(depth * 4, ' ') + qs(tabs[i].label)));
			leaf.push_back(!parent);
			// a tab with rows of its own keeps them on top of the tabs under it
			const int up = tabs[i].parent;
			if (up >= 0 && stackOf[size_t(up)]) {
				QStackedWidget *stack = stackOf[size_t(up)];
				tabPage.push_back(tabPage[size_t(up)]);
				subPage.push_back({stack, stack->addWidget(buildPage(int(i), tabs[i], true))});
				continue;
			}
			if (parent && !hasRows(int(i))) {
				tabPage.push_back(-1);
				subPage.push_back({});
				continue;
			}
			QWidget *page = buildPage(int(i), tabs[i], !parent);
			subPage.push_back({});
			if (parent) {
				stackOf[i] = new QStackedWidget;
				subPage.back() = {stackOf[i], 0};
				auto *rule = new QFrame;
				rule->setFrameShape(QFrame::HLine);
				rule->setFrameShadow(QFrame::Sunken);
				auto *holder = new QWidget;
				auto *v = new QVBoxLayout(holder);
				v->setContentsMargins(0, 0, 0, 0);
				v->addWidget(page);
				v->addWidget(rule);
				v->addWidget(stackOf[i], 1);
				page = holder;
			}
			tabPage.push_back(pages->addWidget(page));
		}
		const int tabWidth = tabList->sizeHintForColumn(0);
		tabList->setFixedWidth(tabWidth + 2 * tabList->frameWidth() + 24);
		for (int i = 0; i < tabList->count(); i++)
			tabList->item(i)->setSizeHint(QSize(tabWidth, tabList->fontMetrics().height() * 2));
		// one width for every number box, so the sliders all end in one column
		int numberWidth = 0;
		for (auto &r : rows)
			if (r->spin)
				numberWidth = std::max(numberWidth, r->spin->sizeHint().width());
		for (auto &r : rows)
			if (r->spin)
				r->spin->setFixedWidth(numberWidth);
		connect(tabList, &QListWidget::currentRowChanged, this, [this](int i) { showTab(i); });

		resetButton = new QPushButton("Reset history");
		resetButton->setAutoDefault(false);
		resetButton->setToolTip("Start every graph over, at Apply. With Keep across restarts on, the history "
								"saved across restarts is deleted too.");
		connect(resetButton, &QPushButton::clicked, this, [this] { resetHistory(); });

		// Cancel stands a little apart from the 2 that save
		auto *cancel = new QPushButton("Cancel");
		applyButton = new QPushButton("&Apply");
		auto *closeButton = new QPushButton("OK");
		cancel->setToolTip("Discard every change and close Settings.");
		applyButton->setToolTip("Save changes now, without closing Settings.");
		closeButton->setToolTip("Save changes and close Settings.");
		closeButton->setDefault(true);
		connect(cancel, &QPushButton::clicked, this, &QDialog::reject);
		connect(applyButton, &QPushButton::clicked, this, [this] { save(); });
		connect(closeButton, &QPushButton::clicked, this, [this] {
			if (save())
				accept();
		});
		int gap = style()->layoutSpacing(QSizePolicy::PushButton, QSizePolicy::PushButton, Qt::Horizontal);
		if (gap < 0)
			gap = 6;
		auto *buttons = new QHBoxLayout;
		buttons->setSpacing(0);
		// under the pages, where Keep across restarts is
		buttons->addSpacing(tabList->minimumWidth() + gap);
		buttons->addWidget(resetButton);
		buttons->addStretch(1);
		buttons->addWidget(cancel);
		buttons->addSpacing(2 * gap);
		buttons->addWidget(applyButton);
		buttons->addSpacing(gap);
		buttons->addWidget(closeButton);
		bottom = {resetButton, cancel, applyButton, closeButton};
		for (auto *b : bottom)
			b->setMinimumHeight(b->sizeHint().height() + BUTTON_GROW);

		auto *body = new QHBoxLayout;
		body->addWidget(tabList);
		body->addWidget(pages, 1);
		auto *outer = new QVBoxLayout(this);
		outer->addLayout(body, 1);
		outer->addSpacing(HEAD_GAP / 2);
		outer->addLayout(buttons);

		tabList->setCurrentRow(0);
		refresh();
		qApp->installEventFilter(this);
		if (list) {
			auto *tick = new QTimer(this);
			connect(tick, &QTimer::timeout, this, [this, tick] {
				tick->setInterval(int(live_sample(*live)));
				for (auto &l : listRows)
					l.space->tick();
			});
			// the first read only primes, and a second one ms later is noise
			tick->start(int(live_sample(*live)));
		}
	}

	// Enter is OK and Esc is Cancel, even from inside a field or on a checkbox.
	bool eventFilter(QObject *obj, QEvent *ev) override {
		if (ev->type() != QEvent::KeyPress)
			return false;
		auto *w = qobject_cast<QWidget *>(obj);
		if (!w || w->window() != this)
			return false;
		int key = static_cast<QKeyEvent *>(ev)->key();
		if (key == Qt::Key_Escape) {
			reject();
			return true;
		}
		if (key != Qt::Key_Return && key != Qt::Key_Enter)
			return false;
		// leaving a field is what commits a half-typed value
		if (QWidget *f = focusWidget())
			f->clearFocus();
		if (save())
			accept();
		return true;
	}

	int tabCount() const { return tabList->count(); }
	std::string tabText(int i) const { return tabList->item(i)->text().toStdString(); }
	std::vector<std::string> shownRows() const {
		std::vector<std::string> ids;
		for (auto &r : rows) {
			QWidget *w = r->control ? r->control : r->label;
			if (w && w->isVisible())
				ids.push_back(r->id());
		}
		return ids;
	}
	std::vector<std::string> buttonTexts() const {
		std::vector<std::string> out;
		for (auto *b : bottom)
			out.push_back(b->text().toStdString());
		return out;
	}
	// the first color box, or the first whole number box at 10 or more that
	// isn't greyed
	QWidget *firstBox(bool number) const {
		for (auto &r : rows) {
			if (number && r->spin && r->spin->isEnabled() && r->info.whole && r->spin->value() >= 10)
				return r->spin;
			if (!number && r->chip)
				return r->line;
		}
		return nullptr;
	}
	bool applyOn() const { return applyButton->isEnabled(); }
	void commitTyped() { takeTyped(); }
	bool isLeaf(int i) const { return leaf[size_t(i)]; }
	void select(int i) { tabList->setCurrentRow(i); }
	std::unique_ptr<ColorPicker> firstPicker() {
		for (auto &r : rows)
			if (r->chip)
				return picker(*r);
		return nullptr;
	}
	std::vector<std::string> greyed() const {
		std::vector<std::string> ids;
		for (auto &r : rows)
			if (r->control && !r->control->isEnabled())
				ids.push_back(r->id());
		return ids;
	}

	// graph width and x, and the width box's x, per graph list row
	std::vector<int> listPlaces() const {
		std::vector<int> out;
		for (auto &l : listRows)
			for (int v : {l.space->width(), l.space->x(), l.width->control->x()})
				out.push_back(v);
		return out;
	}

private:
	Dialog &d;
	rust::Box<Live> live;
	std::vector<std::unique_ptr<Row>> rows;
	// per line of the tab list
	std::vector<int> tabPage;
	std::vector<bool> leaf;
	struct SubPage {
		QStackedWidget *stack = nullptr;
		int index = 0;
	};
	std::vector<SubPage> subPage;
	QPushButton *saveTheme = nullptr;
	QPushButton *renameTheme = nullptr;
	QPushButton *deleteTheme = nullptr;
	QListWidget *tabList = nullptr;
	QStackedWidget *pages = nullptr;
	QPushButton *applyButton = nullptr;
	QPushButton *resetButton = nullptr;
	std::vector<QPushButton *> bottom;

	// One graph per row, first drawn first, in the order they're placed.
	struct ListRow {
		std::string name;
		Row *show = nullptr;
		Row *width = nullptr;
		Row *sample = nullptr;
		Row *time = nullptr;
		LiveGraph *space = nullptr;
		QToolButton *revert = nullptr;
		Gripper *grip = nullptr;
	};
	QWidget *list = nullptr;
	QGridLayout *listGrid = nullptr;
	std::vector<ListRow> listRows;

	// a tab with tabs under it shows its first one
	void showTab(int i) {
		if (i < 0)
			return;
		for (int j = i; j < tabList->count(); j++) {
			if (tabPage[size_t(j)] >= 0) {
				pages->setCurrentIndex(tabPage[size_t(j)]);
				if (auto &s = subPage[size_t(j)]; s.stack)
					s.stack->setCurrentIndex(s.index);
				return;
			}
		}
	}

	QWidget *buildPage(int tab, const TabInfo &info, bool scrolls) {
		auto *page = new QWidget;
		auto *grid = new QGridLayout(page);
		grid->setColumnStretch(1, 1);
		grid->setVerticalSpacing(6);
		int line = 0;
		for (auto &rp : rows) {
			Row &r = *rp;
			if (r.info.tab != tab)
				continue;
			if (r.info.in_list) {
				if (!list)
					grid->addWidget(buildList(), line++, 0, 1, 3);
				continue;
			}
			QString label = qs(r.info.label);
			if (std::string(r.info.control) == "heading") {
				// a heading that only repeats the tab's name says nothing
				if (label == qs(info.label))
					continue;
				auto *h = new QLabel(label);
				QFont f = h->font();
				f.setBold(true);
				h->setFont(f);
				auto *rule = new QFrame;
				rule->setFrameShape(QFrame::HLine);
				rule->setFrameShadow(QFrame::Sunken);
				if (line > 0) {
					r.grid = grid;
					r.gap = line;
					grid->setRowMinimumHeight(line++, HEAD_GAP);
				}
				grid->addWidget(h, line++, 0, 1, 3);
				grid->addWidget(rule, line++, 0, 1, 3);
				r.head = h;
				r.rule = rule;
				continue;
			}
			r.label = new QLabel(label);
			r.label->setContentsMargins(16 * r.info.indent, 0, 12, 0);
			grid->addWidget(r.label, line, 0);
			r.control = buildControl(r);
			grid->addWidget(r.control, line, 1);
			r.revert = new QToolButton;
			r.revert->setIcon(QIcon::fromTheme("edit-undo", style()->standardIcon(QStyle::SP_BrowserReload)));
			r.revert->setAutoRaise(true);
			connect(r.revert, &QToolButton::clicked, this, [this, &r] {
				reset_row(d, r.id());
				refresh();
			});
			grid->addWidget(r.revert, line, 2);
			line++;
		}
		if (!scrolls)
			return page;
		grid->setRowStretch(line, 1);
		auto *scroll = new FitScroll;
		scroll->setWidget(page);
		scroll->setWidgetResizable(true);
		scroll->setFrameShape(QFrame::NoFrame);
		return scroll;
	}

	Row *rowFor(const rust::String &id) {
		for (auto &r : rows)
			if (r->info.id == id)
				return r.get();
		return nullptr;
	}

	// The rows under Graphs as one list: show, a space for the graph, width,
	// sample rate, time axis, revert and a gripper. The grippers write the
	// order.
	QWidget *buildList() {
		list = new QWidget;
		listGrid = new QGridLayout(list);
		listGrid->setContentsMargins(0, 0, 0, 0);
		listGrid->setColumnStretch(5, 1);
		for (auto &g : graph_rows(d)) {
			ListRow lr{std::string(g.name), rowFor(g.show), rowFor(g.width), rowFor(g.sample), rowFor(g.time)};
			if (!lr.show || !lr.width || !lr.sample || !lr.time)
				continue;
			for (Row *r : {lr.show, lr.width, lr.sample, lr.time})
				r->control = buildControl(*r);
			lr.show->check->setText(qs(lr.show->info.label));
			// 0 follows Sample ms default, as Default does in the time axis column
			lr.sample->spin->setSpecialValueText("Default");
			lr.space = new LiveGraph(*live, lr.name);
			lr.space->fit();
			lr.revert = new QToolButton;
			lr.revert->setIcon(QIcon::fromTheme("edit-undo", style()->standardIcon(QStyle::SP_BrowserReload)));
			lr.revert->setAutoRaise(true);
			lr.grip = new Gripper;
			lr.grip->setToolTip("Drag up or down to move this graph, or use Up and Down here. The top one is "
								"drawn first: on the left in a horizontal panel, on top in a vertical one.");
			const std::string name = lr.name;
			connect(lr.revert, &QToolButton::clicked, this, [this, name] {
				for (auto &l : listRows)
					if (l.name == name)
						for (Row *r : {l.show, l.width, l.sample, l.time})
							reset_row(d, r->id());
				refresh();
			});
			lr.grip->dragged = [this, name](QPoint at) { dragTo(name, list->mapFromGlobal(at).y()); };
			lr.grip->stepped = [this, name](int by) { moveBy(name, by); };
			listRows.push_back(lr);
		}
		auto *widthHead = new QLabel("Width %");
		auto *sampleHead = new QLabel("Sample ms");
		auto *timeHead = new QLabel("Time axis");
		widthHead->setToolTip(listRows.empty() ? "" : qs(row_state(d, listRows[0].width->id()).tip));
		sampleHead->setToolTip(listRows.empty() ? "" : qs(row_state(d, listRows[0].sample->id()).tip));
		listGrid->addWidget(widthHead, 0, 2);
		listGrid->addWidget(sampleHead, 0, 3);
		listGrid->addWidget(timeHead, 0, 4);
		placeList();
		return list;
	}

	// puts each row's widgets on its grid line, in listRows' order
	void placeList() {
		for (size_t i = 0; i < listRows.size(); i++) {
			const ListRow &l = listRows[i];
			const int at = int(i) + 1;
			QWidget *cells[] = {
				l.show->control, l.space, l.width->control, l.sample->control, l.time->control, l.revert, l.grip};
			const int cols[] = {0, 1, 2, 3, 4, 6, 7};
			for (int k = 0; k < 7; k++) {
				listGrid->removeWidget(cells[k]);
				// narrower graphs sit left, so the column is the widest's
				listGrid->addWidget(cells[k], at, cols[k], k == 1 ? Qt::AlignLeft | Qt::AlignVCenter : Qt::Alignment());
			}
		}
		// the next drag event reads the new places
		listGrid->activate();
	}

	int listIndex(const std::string &name) const {
		for (size_t i = 0; i < listRows.size(); i++)
			if (listRows[i].name == name)
				return int(i);
		return -1;
	}

	void moveTo(int from, int to) {
		if (from < 0 || to < 0 || from == to || !move_graph(d, size_t(from), size_t(to)))
			return;
		refresh();
	}

	void moveBy(const std::string &name, int by) {
		const int from = listIndex(name);
		moveTo(from, std::clamp(from + by, 0, int(listRows.size()) - 1));
	}

	// `y` is in the list's own coordinates
	void dragTo(const std::string &name, int y) {
		int to = 0;
		for (size_t i = 1; i < listRows.size(); i++)
			if (y >= listGrid->cellRect(int(i) + 1, 0).top())
				to = int(i);
		moveTo(listIndex(name), to);
	}

	// after a drag, a revert or a reload of the order
	void refreshList() {
		std::vector<ListRow> sorted;
		for (auto &g : graph_rows(d)) {
			const int i = listIndex(std::string(g.name));
			if (i >= 0)
				sorted.push_back(listRows[size_t(i)]);
		}
		bool moved = sorted.size() != listRows.size();
		for (size_t i = 0; !moved && i < sorted.size(); i++)
			moved = sorted[i].name != listRows[i].name;
		if (moved) {
			listRows = sorted;
			placeList();
		}
		live_look(*live, d);
		for (auto &l : listRows) {
			l.space->fit();
			bool changed = false;
			for (Row *r : {l.show, l.width, l.sample, l.time})
				changed |= row_state(d, r->id()).can_reset;
			l.revert->setEnabled(changed);
			l.revert->setToolTip(changed ? "Back to the defaults." : "");
		}
	}

	QDoubleSpinBox *numberBox(Row &r) {
		auto *spin = new QDoubleSpinBox;
		spin->setDecimals(r.info.whole ? 0 : 2);
		spin->setRange(std::isfinite(r.info.min) ? r.info.min : -1e9, std::isfinite(r.info.max) ? r.info.max : 1e9);
		spin->setKeyboardTracking(false);
		connect(spin, &QDoubleSpinBox::valueChanged, this, [this, &r](double v) {
			set_number(d, r.id(), v);
			refresh();
		});
		// with keyboard tracking off, the box itself says nothing until it's left
		connect(spin->findChild<QLineEdit *>(), &QLineEdit::textEdited, this, [this] { updateApply(); });
		return spin;
	}

	QWidget *buildControl(Row &r) {
		const std::string c(r.info.control);
		if (c == "checkbox") {
			r.check = new QCheckBox;
			connect(r.check, &QCheckBox::toggled, this, [this, &r](bool on) {
				set_bool(d, r.id(), on);
				refresh();
			});
			return r.check;
		}
		if (c == "slider") {
			r.slider = new QSlider(Qt::Horizontal);
			r.slider->setRange(0, SLIDER_STEPS);
			r.slider->setMinimumWidth(180);
			connect(r.slider, &QSlider::valueChanged, this, [this, &r](int v) {
				set_slider(d, r.id(), double(v) / SLIDER_STEPS);
				refresh();
			});
			r.spin = numberBox(r);
			auto *w = new QWidget;
			auto *h = new QHBoxLayout(w);
			h->setContentsMargins(0, 0, 0, 0);
			h->addWidget(r.slider, 1);
			h->addWidget(r.spin);
			return w;
		}
		if (c == "number") {
			r.spin = numberBox(r);
			return r.spin;
		}
		if (c == "radio") {
			auto *w = new QWidget;
			auto *h = new QHBoxLayout(w);
			h->setContentsMargins(0, 0, 0, 0);
			auto *group = new QButtonGroup(w);
			for (size_t i = 0; i < r.info.options.size(); i++) {
				auto *b = new QRadioButton(qs(r.info.option_labels[i]));
				group->addButton(b);
				r.radios.push_back(b);
				r.keys.push_back(std::string(r.info.options[i]));
				connect(b, &QRadioButton::toggled, this, [this, &r, i](bool on) {
					// the one turned off says so too, and the other one covers it
					if (!on)
						return;
					set_text(d, r.id(), r.keys[i]);
					refresh();
				});
				h->addWidget(b);
			}
			h->addStretch(1);
			return w;
		}
		if (c == "dropdown") {
			r.combo = new QComboBox;
			r.combo->setSizeAdjustPolicy(QComboBox::AdjustToContents);
			connect(r.combo, &QComboBox::activated, this, [this, &r](int i) {
				if (i >= 0 && size_t(i) < r.keys.size())
					set_text(d, r.id(), r.keys[size_t(i)]);
				refresh();
			});
			if (!r.info.chooser)
				return r.combo;
			return themeButtons(r.combo);
		}
		r.line = new QLineEdit;
		connect(r.line, &QLineEdit::editingFinished, this, [this, &r] {
			set_text(d, r.id(), r.line->text().toStdString());
			refresh();
		});
		connect(r.line, &QLineEdit::textEdited, this, [this] { updateApply(); });
		if (c != "color")
			return r.line;

		r.line->setValidator(new QRegularExpressionValidator(QRegularExpression("#?[0-9a-fA-F]{0,6}"), r.line));
		r.line->setMaximumWidth(r.line->fontMetrics().horizontalAdvance("#DDDDDDD") + 16);
		r.chip = new QPushButton;
		r.chip->setFixedSize(36, r.line->sizeHint().height());
		r.chip->setToolTip("Pick a color.");
		connect(r.chip, &QPushButton::clicked, this, [this, &r] { pickColor(r); });
		auto *w = new QWidget;
		auto *h = new QHBoxLayout(w);
		h->setContentsMargins(0, 0, 0, 0);
		h->addWidget(r.chip);
		h->addWidget(r.line);
		h->addStretch(1);
		return w;
	}

	// Like the rest of the window, these take effect at Apply.
	QWidget *themeButtons(QComboBox *combo) {
		saveTheme = new QPushButton("Save");
		auto *saveAs = new QPushButton("Save as...");
		renameTheme = new QPushButton("Rename...");
		deleteTheme = new QPushButton("Delete");
		auto *dice = new QPushButton("Randomize");
		saveTheme->setToolTip("Keep the colors changed here in this theme.");
		saveAs->setToolTip("Keep the colors showing as a new theme of your own.");
		renameTheme->setToolTip("Give this theme another name.");
		deleteTheme->setToolTip("Delete this theme. Default is picked in its place.");
		dice->setToolTip("Random colors for every graph, in one scheme around the User color, with the "
						 "ones to watch kept red and orange. They're changes on this theme, so Save as keeps "
						 "them and picking a theme drops them.");
		for (auto *b : {saveTheme, saveAs, renameTheme, deleteTheme, dice})
			b->setAutoDefault(false);
		connect(saveTheme, &QPushButton::clicked, this, [this] {
			save_theme(d);
			refresh();
		});
		connect(saveAs, &QPushButton::clicked, this, [this] { nameTheme("Save theme as", QString(), false); });
		connect(renameTheme, &QPushButton::clicked, this,
			[this] { nameTheme("Rename theme", qs(theme_name(d)), true); });
		connect(deleteTheme, &QPushButton::clicked, this, [this] {
			const QString name = qs(theme_name(d));
			if (QMessageBox::question(this, "Delete theme", QString("Delete the theme %1?").arg(name)) !=
				QMessageBox::Yes)
				return;
			delete_theme(d);
			refresh();
		});
		connect(dice, &QPushButton::clicked, this, [this] {
			randomize_theme(d);
			refresh();
		});
		auto *w = new QWidget;
		auto *h = new QHBoxLayout(w);
		h->setContentsMargins(0, 0, 0, 0);
		h->addWidget(combo);
		h->addSpacing(12);
		for (auto *b : {saveTheme, saveAs, renameTheme, deleteTheme, dice})
			h->addWidget(b);
		h->addStretch(1);
		return w;
	}

	// asks again after a name that won't do, with the reason
	void nameTheme(const QString &title, QString name, bool rename) {
		for (;;) {
			bool ok = false;
			name = QInputDialog::getText(this, title, "Name:", QLineEdit::Normal, name, &ok);
			if (!ok)
				return;
			const std::string typed = name.toStdString();
			auto err = rename ? rename_theme(d, typed) : save_theme_as(d, typed);
			if (err.empty())
				break;
			QMessageBox::warning(this, title, qs(err));
		}
		refresh();
	}

	std::unique_ptr<ColorPicker> picker(Row &r) {
		if (!pick_open(d, r.id()))
			return nullptr;
		return std::make_unique<ColorPicker>(d, r.label->text(), [this] { refresh(); }, this);
	}

	void pickColor(Row &r) {
		if (auto pick = picker(r))
			pick->exec();
	}

	// the window's own graphs start over now, the panel's at Apply
	void resetHistory() {
		if (row_state(d, "keep_history").on &&
			QMessageBox::question(this, "Reset history",
								  "Keep across restarts is on, so this deletes all past history, not only what's "
								  "on screen. It goes at Apply.\n\nReset history?") != QMessageBox::Yes)
			return;
		reset_history(d);
		live_clear(*live);
		refresh();
	}

	// a box still being typed in, which counts as a change before it's left
	bool typing() const {
		for (auto &r : rows) {
			if (r->line && r->line->isModified())
				return true;
			if (!r->spin)
				continue;
			// a sample rate of 0 shows as Default
			const bool special =
				!r->spin->specialValueText().isEmpty() && r->spin->value() == r->spin->minimum();
			const QString shows =
				special ? r->spin->specialValueText() : r->spin->textFromValue(r->spin->value());
			if (r->spin->text() != shows)
				return true;
		}
		return false;
	}

	void updateApply() {
		// the number boxes set their range before the button is made
		if (applyButton)
			applyButton->setEnabled(is_dirty(d) || typing());
	}

	// Alt+A clicks Apply with the focus left in the box, so nothing else
	// would take what was typed there.
	void takeTyped() {
		bool took = false;
		for (auto &r : rows) {
			if (r->line && r->line->isModified()) {
				r->line->setModified(false);
				set_text(d, r->id(), r->line->text().toStdString());
				took = true;
			}
			// a changed value refreshes through valueChanged
			if (r->spin)
				r->spin->interpretText();
		}
		if (took)
			refresh();
	}

	bool save() {
		takeTyped();
		if (!is_dirty(d))
			return true;
		auto err = apply(d);
		refresh();
		if (err.empty())
			return true;
		QMessageBox::warning(this, "Settings weren't saved", qs(err));
		return false;
	}

	void refresh() {
		for (auto &rp : rows) {
			Row &r = *rp;
			if (r.head) {
				const bool shown = row_state(d, r.id()).shown;
				r.head->setVisible(shown);
				r.rule->setVisible(shown);
				if (r.grid)
					r.grid->setRowMinimumHeight(r.gap, shown ? HEAD_GAP : 0);
			}
			// the graph list's rows have a control and no label or revert
			if (!r.control)
				continue;
			auto st = row_state(d, r.id());
			for (QWidget *w : {static_cast<QWidget *>(r.label), r.control, static_cast<QWidget *>(r.revert)})
				if (w)
					w->setVisible(st.shown);
			// greyed while a switch above it is off, but its revert still works
			for (QWidget *w : {static_cast<QWidget *>(r.label), r.control})
				if (w)
					w->setEnabled(st.counts);
			QString tip = qs(st.tip);
			QString text = qs(st.text);
			if (r.label) {
				r.label->setToolTip(tip);
				r.revert->setEnabled(st.can_reset);
				r.revert->setToolTip(st.can_reset ? "Back to the default." : "");
			}
			if (r.check) {
				QSignalBlocker b(r.check);
				r.check->setChecked(st.on);
				r.check->setToolTip(tip);
			}
			if (r.slider) {
				QSignalBlocker b(r.slider);
				r.slider->setValue(int(std::lround(st.slider * SLIDER_STEPS)));
				r.slider->setToolTip(tip);
			}
			if (r.spin) {
				QSignalBlocker b(r.spin);
				r.spin->setValue(st.number);
				r.spin->setToolTip(tip);
			}
			if (r.combo) {
				QSignalBlocker b(r.combo);
				std::vector<std::string> keys;
				for (auto &k : st.options)
					keys.push_back(std::string(k));
				if (keys != r.keys) {
					r.keys = keys;
					r.combo->clear();
					for (auto &l : st.option_labels)
						r.combo->addItem(qs(l));
				}
				for (size_t i = 0; i < r.keys.size(); i++) {
					const bool on = QString::fromStdString(r.keys[i]) == text;
					// a theme with changes on top says "Name *"
					r.combo->setItemText(int(i), on && !st.choice.empty() ? qs(st.choice) : qs(st.option_labels[i]));
					if (on)
						r.combo->setCurrentIndex(int(i));
				}
				r.combo->setToolTip(tip);
			}
			for (size_t i = 0; i < r.radios.size(); i++) {
				QSignalBlocker b(r.radios[i]);
				r.radios[i]->setChecked(QString::fromStdString(r.keys[i]) == text);
				r.radios[i]->setToolTip(tip);
			}
			if (r.line) {
				QSignalBlocker b(r.line);
				r.line->setText(text);
				r.line->setToolTip(tip);
			}
			if (r.chip)
				r.chip->setStyleSheet(QString("background-color: %1; border: 1px solid palette(mid);").arg(text));
		}
		if (list)
			refreshList();
		if (saveTheme) {
			auto b = theme_buttons(d);
			saveTheme->setEnabled(b.save);
			renameTheme->setEnabled(b.rename);
			deleteTheme->setEnabled(b.remove);
		}
		updateApply();
		resetButton->setEnabled(!reset_pending(d));
	}
};

} // namespace

std::int32_t run_dialog(Dialog &d, rust::Str snapshot) {
	static int argc = 1;
	static char name[] = "tukzmon-settings";
	static char *argv[] = {name, nullptr};
	QApplication app(argc, argv);
	QApplication::setApplicationName("tukzmon-settings");
	TipWrap tipWrap;
	app.installEventFilter(&tipWrap);
	app.setStyle(new BoxStyle);
	Window w(d);
	QSize size = w.sizeHint().expandedTo(QSize(560, 420));
	if (const QScreen *screen = w.screen())
		size = size.boundedTo(screen->availableGeometry().size());
	w.resize(size);
	if (snapshot.empty())
		return w.exec() == QDialog::Accepted ? 0 : 1;

	// one picture per tab with rows, as base-1.png, base-2.png, and so on, then
	// the color picker
	QString base = QString::fromUtf8(snapshot.data(), int(snapshot.size()));
	if (base.endsWith(".png"))
		base.chop(4);
	w.show();
	int n = 0;
	for (int i = 0; i < w.tabCount(); i++) {
		if (!w.isLeaf(i))
			continue;
		w.select(i);
		app.processEvents();
		if (!w.grab().save(QString("%1-%2.png").arg(base).arg(++n)))
			return 3;
	}
	if (auto pick = w.firstPicker()) {
		pick->show();
		app.processEvents();
		const bool saved = pick->grab().save(QString("%1-%2.png").arg(base).arg(++n));
		pick->reject();
		if (!saved)
			return 3;
	}
	return 0;
}

rust::Vec<rust::String> greyed_rows(Dialog &d) {
	static int argc = 1;
	static char name[] = "tukzmon-settings";
	static char *argv[] = {name, nullptr};
	qputenv("QT_QPA_PLATFORM", "offscreen");
	QApplication app(argc, argv);
	Window w(d);
	rust::Vec<rust::String> ids;
	for (auto &id : w.greyed())
		ids.push_back(rust::String(id));
	return ids;
}

rust::Vec<std::int32_t> graph_list_places(Dialog &d) {
	static int argc = 1;
	static char name[] = "tukzmon-settings";
	static char *argv[] = {name, nullptr};
	qputenv("QT_QPA_PLATFORM", "offscreen");
	QApplication app(argc, argv);
	Window w(d);
	w.show();
	app.processEvents();
	rust::Vec<std::int32_t> out;
	for (int v : w.listPlaces())
		out.push_back(v);
	return out;
}

rust::Vec<TabShown> tabs_shown(Dialog &d) {
	static int argc = 1;
	static char name[] = "tukzmon-settings";
	static char *argv[] = {name, nullptr};
	qputenv("QT_QPA_PLATFORM", "offscreen");
	QApplication app(argc, argv);
	Window w(d);
	w.show();
	rust::Vec<TabShown> out;
	for (int i = 0; i < w.tabCount(); i++) {
		w.select(i);
		app.processEvents();
		TabShown t;
		t.label = rust::String(w.tabText(i));
		for (auto &id : w.shownRows())
			t.rows.push_back(rust::String(id));
		out.push_back(std::move(t));
	}
	return out;
}

rust::Vec<rust::String> bottom_buttons(Dialog &d) {
	static int argc = 1;
	static char name[] = "tukzmon-settings";
	static char *argv[] = {name, nullptr};
	qputenv("QT_QPA_PLATFORM", "offscreen");
	QApplication app(argc, argv);
	Window w(d);
	rust::Vec<rust::String> out;
	for (auto &t : w.buttonTexts())
		out.push_back(rust::String(t));
	return out;
}

namespace {
void press(QWidget *w, int key, const QString &text = QString()) {
	QKeyEvent down(QEvent::KeyPress, key, Qt::NoModifier, text);
	QKeyEvent up(QEvent::KeyRelease, key, Qt::NoModifier, text);
	QApplication::sendEvent(w, &down);
	QApplication::sendEvent(w, &up);
}
} // namespace

rust::Vec<bool> apply_while_typing(Dialog &d, bool number) {
	static int argc = 1;
	static char name[] = "tukzmon-settings";
	static char *argv[] = {name, nullptr};
	qputenv("QT_QPA_PLATFORM", "offscreen");
	QApplication app(argc, argv);
	Window w(d);
	QWidget *box = w.firstBox(number);
	if (!box)
		return {};
	rust::Vec<bool> out;
	out.push_back(w.applyOn());
	// the last digit typed over with another, which stays in range
	auto *spin = qobject_cast<QDoubleSpinBox *>(box);
	const QString text = spin ? spin->text() : static_cast<QLineEdit *>(box)->text();
	const QChar digit = text.back() == '1' ? '2' : '1';
	press(box, Qt::Key_End);
	press(box, Qt::Key_Backspace);
	press(box, digit == '1' ? Qt::Key_1 : Qt::Key_2, digit);
	out.push_back(w.applyOn());
	w.commitTyped();
	out.push_back(is_dirty(d));
	return out;
}

double empty_box_contrast(bool dark) {
	static int argc = 1;
	static char name[] = "tukzmon-settings";
	static char *argv[] = {name, nullptr};
	qputenv("QT_QPA_PLATFORM", "offscreen");
	QApplication app(argc, argv);
	app.setStyle(new BoxStyle);
	QPalette pal = app.palette();
	// close to the dark themes where the box went missing
	if (dark) {
		pal.setColor(QPalette::Window, QColor("#2b2b2b"));
		pal.setColor(QPalette::WindowText, QColor("#dcdcdc"));
		pal.setColor(QPalette::Base, QColor("#2f2f2f"));
		pal.setColor(QPalette::Text, QColor("#dcdcdc"));
		pal.setColor(QPalette::Button, QColor("#353535"));
	}
	app.setPalette(pal);
	QCheckBox box;
	const QImage img = box.grab().toImage();
	auto lum = [](QColor c) {
		auto lin = [](double v) { return v <= 0.04045 ? v / 12.92 : std::pow((v + 0.055) / 1.055, 2.4); };
		return 0.2126 * lin(c.redF()) + 0.7152 * lin(c.greenF()) + 0.0722 * lin(c.blueF());
	};
	const double back = lum(pal.color(QPalette::Window));
	double best = 1;
	for (int y = 0; y < img.height(); y++)
		for (int x = 0; x < img.width(); x++) {
			const double l = lum(img.pixelColor(x, y));
			best = std::max(best, (std::max(l, back) + 0.05) / (std::min(l, back) + 0.05));
		}
	return best;
}

} // namespace tukzmon
